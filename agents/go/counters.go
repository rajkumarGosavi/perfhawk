package agent

import (
	"encoding/json"
	"math"
	"net/http"
	"runtime/metrics"
	"strconv"
	"time"
)

const counterInterval = time.Second

type counterPoint struct {
	TNs   uint64  `json:"t_ns"`
	Value float64 `json:"value"`
}

type counterSeries struct {
	Name        string         `json:"name"`
	DisplayName string         `json:"display_name"`
	Unit        string         `json:"unit"`
	Kind        string         `json:"kind"`
	Points      []counterPoint `json:"points"`
}

type counterDoc struct {
	Schema      int              `json:"schema"`
	Runtime     string           `json:"runtime"`
	StartUnixNs int64            `json:"start_unix_ns"`
	DurationNs  uint64           `json:"duration_ns"`
	IntervalNs  uint64           `json:"interval_ns"`
	Series      []*counterSeries `json:"series"`
	Events      []struct{}       `json:"events"`
	Notes       []string         `json:"notes"`
}

// counterGauges are runtime/metrics values reported as-is each interval.
var counterGauges = []struct{ metric, display, unit string }{
	{"/sched/goroutines:goroutines", "Goroutines", "count"},
	{"/sched/goroutines/runnable:goroutines", "Runnable goroutines", "count"},
	{"/sched/threads/total:threads", "OS threads", "count"},
	{"/sched/gomaxprocs:threads", "GOMAXPROCS", "count"},
}

const (
	mutexWaitMetric = "/sync/mutex/wait/total:seconds"
	schedLatMetric  = "/sched/latencies:seconds"
	mutexWaitName   = "mutex-wait-ms"
	schedLatP99Name = "sched-latency-p99-ms"
)

// p99Delta returns the 99th percentile, in milliseconds, of the samples added
// between two readings of the same histogram. It reports the upper bound of the
// bucket holding the percentile (the lower bound for the +Inf overflow bucket).
// ok is false when no samples were added.
func p99Delta(prev, cur *metrics.Float64Histogram) (float64, bool) {
	if cur == nil || len(cur.Counts) == 0 {
		return 0, false
	}
	delta := make([]uint64, len(cur.Counts))
	var total uint64
	for i, c := range cur.Counts {
		var p uint64
		if prev != nil && i < len(prev.Counts) {
			p = prev.Counts[i]
		}
		if c > p {
			delta[i] = c - p
			total += delta[i]
		}
	}
	if total == 0 {
		return 0, false
	}
	target := uint64(math.Ceil(0.99 * float64(total)))
	var seen uint64
	for i, d := range delta {
		seen += d
		if seen >= target {
			v := cur.Buckets[i+1]
			if math.IsInf(v, 0) {
				v = cur.Buckets[i]
			}
			if math.IsInf(v, 0) {
				return 0, false
			}
			return v * 1000, true
		}
	}
	return 0, false
}

// countersHandler serves /debug/perfhawk/counters?seconds=N: it reads
// runtime/metrics once per second for N seconds. Metrics this Go version
// lacks are skipped with a note.
func countersHandler(w http.ResponseWriter, r *http.Request) {
	secs, err := strconv.Atoi(r.URL.Query().Get("seconds"))
	if err != nil || secs < 1 || secs > 600 {
		http.Error(w, "seconds must be 1..600", http.StatusBadRequest)
		return
	}
	if !tryLock("counters") {
		http.Error(w, "a counters capture is already running", http.StatusConflict)
		return
	}
	defer unlock("counters")

	names := make([]string, 0, len(counterGauges)+2)
	for _, g := range counterGauges {
		names = append(names, g.metric)
	}
	names = append(names, mutexWaitMetric, schedLatMetric)
	samples := make([]metrics.Sample, len(names))
	for i, n := range names {
		samples[i].Name = n
	}
	metrics.Read(samples)

	t0 := time.Now()
	doc := counterDoc{
		Schema: 1, Runtime: "go", StartUnixNs: t0.UnixNano(), IntervalNs: uint64(counterInterval),
		Series: []*counterSeries{}, Events: []struct{}{}, Notes: []string{},
	}
	present := make([]*counterSeries, len(names))
	for i, s := range samples {
		if s.Value.Kind() == metrics.KindBad {
			doc.Notes = append(doc.Notes, s.Name+" not available in this Go version")
			continue
		}
		var cs *counterSeries
		switch {
		case i < len(counterGauges):
			g := counterGauges[i]
			cs = &counterSeries{Name: g.metric, DisplayName: g.display, Unit: g.unit, Kind: "gauge"}
		case s.Name == mutexWaitMetric:
			cs = &counterSeries{Name: mutexWaitName, DisplayName: "Mutex wait", Unit: "ms", Kind: "rate"}
		case s.Name == schedLatMetric:
			cs = &counterSeries{Name: schedLatP99Name, DisplayName: "Scheduler latency p99", Unit: "ms", Kind: "gauge"}
		}
		cs.Points = []counterPoint{}
		present[i] = cs
		doc.Series = append(doc.Series, cs)
	}

	var prevMutex float64
	var prevHist *metrics.Float64Histogram
	snapshot := func() {
		for _, s := range samples {
			switch s.Value.Kind() {
			case metrics.KindFloat64:
				if s.Name == mutexWaitMetric {
					prevMutex = s.Value.Float64()
				}
			case metrics.KindFloat64Histogram:
				h := s.Value.Float64Histogram()
				prevHist = &metrics.Float64Histogram{Counts: append([]uint64(nil), h.Counts...), Buckets: h.Buckets}
			}
		}
	}
	snapshot()
	add := func(cs *counterSeries, t uint64, v float64) {
		if n := len(cs.Points); n > 0 && cs.Points[n-1].TNs >= t {
			return
		}
		if math.IsNaN(v) || math.IsInf(v, 0) {
			return
		}
		cs.Points = append(cs.Points, counterPoint{TNs: t, Value: v})
	}
	poll := func(now time.Time, first bool) {
		t := uint64(now.Sub(t0))
		if !first {
			metrics.Read(samples)
		}
		for i, s := range samples {
			cs := present[i]
			if cs == nil {
				continue
			}
			switch s.Value.Kind() {
			case metrics.KindUint64:
				add(cs, t, float64(s.Value.Uint64()))
			case metrics.KindFloat64:
				if !first {
					v := s.Value.Float64()
					add(cs, t, (v-prevMutex)*1000)
				}
			case metrics.KindFloat64Histogram:
				if !first {
					if p, ok := p99Delta(prevHist, s.Value.Float64Histogram()); ok {
						add(cs, t, p)
					}
				}
			}
		}
		if !first {
			snapshot()
		}
	}
	poll(t0, true)

	tick := time.NewTicker(counterInterval)
	defer tick.Stop()
	end := time.NewTimer(time.Duration(secs) * time.Second)
	defer end.Stop()
loop:
	for {
		select {
		case now := <-tick.C:
			poll(now, false)
		case <-end.C:
			poll(time.Now(), false)
			break loop
		case <-r.Context().Done():
			return
		}
	}
	doc.DurationNs = uint64(time.Since(t0))
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(doc)
}
