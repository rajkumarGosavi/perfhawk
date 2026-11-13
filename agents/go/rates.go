package agent

import (
	"bytes"
	"net/http"
	"runtime"
	"runtime/pprof"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/google/pprof/profile"
)

// blockRate tracks the block profile rate chosen by the application through
// SetBlockProfileRate. The runtime has no getter, so the agent can only
// restore what it was told about; otherwise it assumes 0 (disabled).
var blockRate atomic.Int64

// SetBlockProfileRate is runtime.SetBlockProfileRate that also records the
// value, so a temporary prism capture restores it afterwards. Applications
// that enable block profiling themselves should call this instead.
func SetBlockProfileRate(rate int) {
	blockRate.Store(int64(rate))
	runtime.SetBlockProfileRate(rate)
}

const (
	capturedMutexFraction = 5
	capturedBlockRateNs   = 10_000
)

var (
	busyMu sync.Mutex
	busy   = map[string]bool{}
)

func tryLock(kind string) bool {
	busyMu.Lock()
	defer busyMu.Unlock()
	if busy[kind] {
		return false
	}
	busy[kind] = true
	return true
}

func unlock(kind string) {
	busyMu.Lock()
	delete(busy, kind)
	busyMu.Unlock()
}

func snapshot(name string) (*profile.Profile, error) {
	var buf bytes.Buffer
	if err := pprof.Lookup(name).WriteTo(&buf, 0); err != nil {
		return nil, err
	}
	return profile.Parse(&buf)
}

// contentionHandler serves /debug/prism/contention?seconds=N&kind=mutex|block:
// it raises the sampling rate, waits, and returns the profile delta. The
// previous rate is restored on every exit path, including a cancelled request.
func contentionHandler(w http.ResponseWriter, r *http.Request) {
	kind := r.URL.Query().Get("kind")
	if kind != "mutex" && kind != "block" {
		http.Error(w, "kind must be mutex or block", http.StatusBadRequest)
		return
	}
	secs, err := strconv.Atoi(r.URL.Query().Get("seconds"))
	if err != nil || secs < 1 || secs > 600 {
		http.Error(w, "seconds must be 1..600", http.StatusBadRequest)
		return
	}
	if !tryLock(kind) {
		http.Error(w, "a "+kind+" capture is already running", http.StatusConflict)
		return
	}
	defer unlock(kind)

	switch kind {
	case "mutex":
		prev := runtime.SetMutexProfileFraction(capturedMutexFraction)
		defer runtime.SetMutexProfileFraction(prev)
	case "block":
		runtime.SetBlockProfileRate(capturedBlockRateNs)
		defer runtime.SetBlockProfileRate(int(blockRate.Load()))
	}

	before, err := snapshot(kind)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	select {
	case <-time.After(time.Duration(secs) * time.Second):
	case <-r.Context().Done():
		return
	}
	after, err := snapshot(kind)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	before.Scale(-1)
	delta, err := profile.Merge([]*profile.Profile{after, before})
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	// Drop samples that did not change during the window.
	kept := delta.Sample[:0]
	for _, s := range delta.Sample {
		for _, v := range s.Value {
			if v != 0 {
				kept = append(kept, s)
				break
			}
		}
	}
	delta.Sample = kept
	w.Header().Set("Content-Type", "application/octet-stream")
	_ = delta.Write(w)
}
