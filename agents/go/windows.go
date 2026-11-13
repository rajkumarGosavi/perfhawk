package agent

import (
	"bytes"
	"net/http"
	"runtime/pprof"
	"strconv"
	"time"

	"github.com/google/pprof/profile"
)

// BucketLabel is the numeric sample label that carries a window's index; the
// perfhawk client turns it into a timeline whose bucket is one window wide.
const BucketLabel = "perfhawk.bucket"

const (
	defaultWindowMs = 250
	minWindowMs     = 50
	maxWindowMs     = 5000
)

// cpuWindowsHandler serves /debug/perfhawk/cpu-windows?seconds=N&window_ms=W: it
// takes back-to-back short CPU profiles, labels every sample with the index of
// its window, and returns them merged into one pprof (samples from different
// windows stay separate because their labels differ). A cancelled request
// stops early and returns what was collected; the CPU profiler is always
// released.
func cpuWindowsHandler(w http.ResponseWriter, r *http.Request) {
	secs, err := strconv.Atoi(r.URL.Query().Get("seconds"))
	if err != nil || secs < 1 || secs > 600 {
		http.Error(w, "seconds must be 1..600", http.StatusBadRequest)
		return
	}
	windowMs := defaultWindowMs
	if v := r.URL.Query().Get("window_ms"); v != "" {
		windowMs, err = strconv.Atoi(v)
		if err != nil || windowMs < minWindowMs || windowMs > maxWindowMs {
			http.Error(w, "window_ms must be 50..5000", http.StatusBadRequest)
			return
		}
	}
	if !tryLock("cpu") {
		http.Error(w, "a CPU capture is already running", http.StatusConflict)
		return
	}
	defer unlock("cpu")

	deadline := time.Now().Add(time.Duration(secs) * time.Second)
	window := time.Duration(windowMs) * time.Millisecond
	var parts []*profile.Profile
	for idx := 0; time.Now().Before(deadline); idx++ {
		var buf bytes.Buffer
		if err := pprof.StartCPUProfile(&buf); err != nil {
			if idx == 0 {
				http.Error(w, "cannot start a CPU profile: "+err.Error(), http.StatusConflict)
				return
			}
			break
		}
		wait := window
		if rem := time.Until(deadline); rem < wait {
			wait = rem
		}
		cancelled := false
		select {
		case <-time.After(wait):
		case <-r.Context().Done():
			cancelled = true
		}
		pprof.StopCPUProfile()
		if p, err := profile.ParseData(buf.Bytes()); err == nil {
			for _, s := range p.Sample {
				s.NumLabel = map[string][]int64{BucketLabel: {int64(idx)}}
				s.NumUnit = map[string][]string{BucketLabel: {"window"}}
			}
			parts = append(parts, p)
		}
		if cancelled {
			return
		}
	}
	if len(parts) == 0 {
		http.Error(w, "no CPU profile windows were collected", http.StatusInternalServerError)
		return
	}
	merged, err := profile.Merge(parts)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/octet-stream")
	_ = merged.Write(w)
}
