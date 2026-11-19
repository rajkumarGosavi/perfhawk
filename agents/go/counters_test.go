package agent

import (
	"context"
	"encoding/json"
	"math"
	"net/http"
	"runtime/metrics"
	"testing"
	"time"
)

type testCounterDoc struct {
	Schema     int    `json:"schema"`
	Runtime    string `json:"runtime"`
	IntervalNs uint64 `json:"interval_ns"`
	Series     []struct {
		Name   string `json:"name"`
		Kind   string `json:"kind"`
		Points []struct {
			TNs   uint64  `json:"t_ns"`
			Value float64 `json:"value"`
		} `json:"points"`
	} `json:"series"`
	Events []json.RawMessage `json:"events"`
	Notes  []string          `json:"notes"`
}

func TestCountersEndpointReturnsSeries(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	raw := get(t, "http://"+a.Addr()+"/debug/perfhawk/counters?seconds=2")
	var d testCounterDoc
	if err := json.Unmarshal(raw, &d); err != nil {
		t.Fatal(err)
	}
	if d.Schema != 1 || d.Runtime != "go" || d.IntervalNs != uint64(time.Second) {
		t.Fatalf("header = %+v", d)
	}
	if d.Events == nil {
		t.Fatal("events must be [] not null")
	}
	found := false
	for _, s := range d.Series {
		for i := 1; i < len(s.Points); i++ {
			if s.Points[i].TNs <= s.Points[i-1].TNs {
				t.Fatalf("%s points not ascending", s.Name)
			}
		}
		if s.Name == "/sched/goroutines:goroutines" {
			found = true
			if s.Kind != "gauge" || len(s.Points) < 2 || s.Points[0].Value < 1 {
				t.Fatalf("goroutines series = %+v", s)
			}
		}
		if s.Name == "mutex-wait-ms" && s.Kind != "rate" {
			t.Fatalf("mutex-wait-ms kind = %s", s.Kind)
		}
	}
	if !found {
		t.Fatalf("no goroutines series in %s", raw)
	}
}

func TestCountersEndpointBadSeconds(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	for _, q := range []string{"", "?seconds=0", "?seconds=601", "?seconds=x"} {
		resp, err := http.Get("http://" + a.Addr() + "/debug/perfhawk/counters" + q)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		if resp.StatusCode != 400 {
			t.Fatalf("%q: status %d, want 400", q, resp.StatusCode)
		}
	}
}

func TestCountersEndpointConflict(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	ctx, cancel := context.WithCancel(context.Background())
	req, _ := http.NewRequestWithContext(ctx, "GET", "http://"+a.Addr()+"/debug/perfhawk/counters?seconds=30", nil)
	done := make(chan struct{})
	go func() {
		defer close(done)
		if resp, err := http.DefaultClient.Do(req); err == nil {
			resp.Body.Close()
		}
	}()
	time.Sleep(300 * time.Millisecond)
	resp, err := http.Get("http://" + a.Addr() + "/debug/perfhawk/counters?seconds=1")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != 409 {
		t.Fatalf("second capture status %d, want 409", resp.StatusCode)
	}
	cancel()
	<-done
}

func TestCountersP99FromHistogramDelta(t *testing.T) {
	buckets := []float64{0, 0.001, 0.002, 0.010, 0.020, math.Inf(1)}
	prev := &metrics.Float64Histogram{Counts: []uint64{5, 0, 0, 0, 0}, Buckets: buckets}
	// 100 new samples: 98 in [0,1ms), 2 in [10ms,20ms) -> p99 falls in the 10-20 ms bucket.
	cur := &metrics.Float64Histogram{Counts: []uint64{103, 0, 0, 2, 0}, Buckets: buckets}
	p, ok := p99Delta(prev, cur)
	if !ok || p < 10 || p > 20 {
		t.Fatalf("p99 = %v ms ok=%v, want within 10..20", p, ok)
	}
	if _, ok := p99Delta(cur, cur); ok {
		t.Fatal("no new samples must report ok=false")
	}
	// Overflow bucket upper bound is +Inf: report its lower bound, never Inf.
	over := &metrics.Float64Histogram{Counts: []uint64{103, 0, 0, 2, 50}, Buckets: buckets}
	if p, ok := p99Delta(cur, over); !ok || math.IsInf(p, 0) || p != 20 {
		t.Fatalf("overflow p99 = %v ok=%v, want 20", p, ok)
	}
}
