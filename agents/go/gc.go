package agent

import (
	"encoding/json"
	"net/http"
	"runtime"
	"runtime/debug"
	"runtime/metrics"
	"sort"
	"strconv"
	"time"
)

const gcPollInterval = 50 * time.Millisecond

type gcEvent struct {
	ID         uint32 `json:"id"`
	StartNs    uint64 `json:"start_ns"`
	Kind       string `json:"kind"`
	Reason     string `json:"reason"`
	Induced    bool   `json:"induced"`
	PauseNs    uint64 `json:"pause_ns"`
	DurationNs uint64 `json:"duration_ns"`
}

type gcHeapPoint struct {
	TNs        uint64 `json:"t_ns"`
	TotalBytes uint64 `json:"total_bytes"`
}

type gcAlloc struct {
	BucketNs uint64   `json:"bucket_ns"`
	Bytes    []uint64 `json:"bytes"`
}

// gcDoc is the perfhawk gc.json (schema 1). Go does not know generations,
// the heap size before a collection or promoted bytes, so those are absent.
type gcDoc struct {
	Schema      int           `json:"schema"`
	Runtime     string        `json:"runtime"`
	StartUnixNs int64         `json:"start_unix_ns"`
	DurationNs  uint64        `json:"duration_ns"`
	Fidelity    string        `json:"fidelity"`
	Events      []gcEvent     `json:"events"`
	Heap        []gcHeapPoint `json:"heap"`
	Alloc       gcAlloc       `json:"alloc"`
	Notes       []string      `json:"notes"`
}

var gcSamples = []metrics.Sample{
	{Name: "/gc/cycles/forced:gc-cycles"},
	{Name: "/gc/heap/live:bytes"},
	{Name: "/gc/heap/allocs:bytes"},
}

func readGCMetrics() (forced uint64, live uint64, haveLive bool, allocs uint64) {
	s := make([]metrics.Sample, len(gcSamples))
	copy(s, gcSamples)
	metrics.Read(s)
	if s[0].Value.Kind() == metrics.KindUint64 {
		forced = s[0].Value.Uint64()
	}
	if s[1].Value.Kind() == metrics.KindUint64 {
		live, haveLive = s[1].Value.Uint64(), true
	}
	if s[2].Value.Kind() == metrics.KindUint64 {
		allocs = s[2].Value.Uint64()
	}
	return
}

// gcHandler serves /debug/perfhawk/gc?seconds=N: it polls the runtime's GC
// statistics (no stop-the-world, unlike runtime.ReadMemStats) every 50 ms for
// N seconds and returns the collections that happened inside the window.
func gcHandler(w http.ResponseWriter, r *http.Request) {
	secs, err := strconv.Atoi(r.URL.Query().Get("seconds"))
	if err != nil || secs < 1 || secs > 600 {
		http.Error(w, "seconds must be 1..600", http.StatusBadRequest)
		return
	}
	if !tryLock("gc") {
		http.Error(w, "a gc capture is already running", http.StatusConflict)
		return
	}
	defer unlock("gc")

	var stats debug.GCStats
	debug.ReadGCStats(&stats)
	lastNumGC := stats.NumGC
	lastForced, _, _, lastAllocs := readGCMetrics()
	t0 := time.Now()
	doc := gcDoc{
		Schema: 1, Runtime: "go", StartUnixNs: t0.UnixNano(), Fidelity: "approximate",
		Events: []gcEvent{}, Heap: []gcHeapPoint{}, Alloc: gcAlloc{BucketNs: uint64(gcPollInterval), Bytes: []uint64{}},
		Notes: []string{"Go GC duration is not measured; duration equals pause time. Reasons and generations are not reported by the Go runtime."},
	}
	if runtime.GOOS == "windows" {
		doc.Notes = append(doc.Notes, "On Windows the Go runtime clock is coarse (about a millisecond or more), so short pauses can read as 0 ns.")
	}
	lost := 0
	pendingForced := 0 // forced cycles seen but not yet matched to a finished collection
	nextID := uint32(1)

	poll := func(now time.Time) {
		elapsed := now.Sub(t0)
		debug.ReadGCStats(&stats)
		forced, live, haveLive, allocs := readGCMetrics()
		// The forced counter moves when a cycle starts, NumGC when it ends, so the two can
		// be seen in different polls: carry the count until events arrive to claim it.
		pendingForced += int(forced - lastForced)
		lastForced = forced
		if n := int(stats.NumGC - lastNumGC); n > 0 {
			have := len(stats.Pause)
			if len(stats.PauseEnd) < have {
				have = len(stats.PauseEnd)
			}
			take := n
			if take > have {
				lost += take - have
				take = have
			}
			batch := make([]gcEvent, 0, take)
			for i := take - 1; i >= 0; i-- { // Pause is newest first
				pause := uint64(stats.Pause[i])
				end := stats.PauseEnd[i].Sub(t0)
				var start uint64
				if end > time.Duration(pause) {
					start = uint64(end) - pause
				}
				batch = append(batch, gcEvent{ID: nextID, StartNs: start, Kind: "concurrent", Reason: "other", PauseNs: pause, DurationNs: pause})
				nextID++
			}
			sort.SliceStable(batch, func(a, b int) bool { return batch[a].StartNs < batch[b].StartNs })
			// runtime.GC() calls advance the forced counter; attribute them to the latest events.
			for i := len(batch) - 1; i >= 0 && pendingForced > 0; i, pendingForced = i-1, pendingForced-1 {
				batch[i].Induced, batch[i].Reason = true, "induced"
			}
			if len(doc.Events) > 0 && len(batch) > 0 && batch[0].StartNs < doc.Events[len(doc.Events)-1].StartNs {
				batch[0].StartNs = doc.Events[len(doc.Events)-1].StartNs
			}
			doc.Events = append(doc.Events, batch...)
			lastNumGC = stats.NumGC
		}
		if haveLive {
			doc.Heap = append(doc.Heap, gcHeapPoint{TNs: uint64(elapsed), TotalBytes: live})
		}
		idx := int(elapsed / gcPollInterval)
		for len(doc.Alloc.Bytes) <= idx {
			doc.Alloc.Bytes = append(doc.Alloc.Bytes, 0)
		}
		if allocs >= lastAllocs {
			doc.Alloc.Bytes[idx] += allocs - lastAllocs
		}
		lastAllocs = allocs
	}

	tick := time.NewTicker(gcPollInterval)
	defer tick.Stop()
	end := time.NewTimer(time.Duration(secs) * time.Second)
	defer end.Stop()
loop:
	for {
		select {
		case now := <-tick.C:
			poll(now)
		case <-end.C:
			poll(time.Now())
			break loop
		case <-r.Context().Done():
			return
		}
	}
	doc.DurationNs = uint64(time.Since(t0))
	if lost > 0 {
		doc.Notes = append(doc.Notes, strconv.Itoa(lost)+" GCs lost between polls (more than 256 collections per poll)")
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(doc)
}
