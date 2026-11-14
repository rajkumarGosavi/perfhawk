// loadgen is a small dependency-free HTTP load generator for driving the sample apps
// while perfhawk records them.
//
//	go run ./tools/loadgen -url http://127.0.0.1:8090/api/health,http://127.0.0.1:8090/api/collections/posts/records -c 32 -d 30s
//
// Several comma-separated URLs are hit round-robin. Use -header "K: V" (repeatable),
// -method and -body for non-GET traffic. It prints throughput, errors and latency
// percentiles, and exits non-zero if every request failed.
package main

import (
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type headers []string

func (h *headers) String() string     { return strings.Join(*h, "; ") }
func (h *headers) Set(v string) error { *h = append(*h, v); return nil }

func main() {
	urls := flag.String("url", "", "comma-separated URLs, hit round-robin (required)")
	conc := flag.Int("c", 16, "concurrent workers")
	dur := flag.Duration("d", 30*time.Second, "how long to run")
	method := flag.String("method", "GET", "HTTP method")
	body := flag.String("body", "", "request body (sent with every request)")
	bodyFile := flag.String("body-file", "", "read the request body from this file (avoids shell quoting issues)")
	var hdrs headers
	flag.Var(&hdrs, "header", `extra header "Key: Value" (repeatable)`)
	flag.Parse()

	targets := strings.FieldsFunc(*urls, func(r rune) bool { return r == ',' })
	if len(targets) == 0 || *conc < 1 {
		fmt.Fprintln(os.Stderr, "usage: loadgen -url <u1,u2,...> [-c N] [-d 30s] [-method M] [-body S|-body-file F] [-header 'K: V']")
		os.Exit(2)
	}

	if *bodyFile != "" {
		b, err := os.ReadFile(*bodyFile)
		if err != nil {
			fmt.Fprintln(os.Stderr, "loadgen:", err)
			os.Exit(2)
		}
		*body = string(b)
	}

	client := &http.Client{
		Timeout: 30 * time.Second,
		Transport: &http.Transport{
			MaxIdleConns:        *conc * 2,
			MaxIdleConnsPerHost: *conc * 2,
		},
	}

	var (
		next     atomic.Uint64
		failed   atomic.Uint64
		mu       sync.Mutex
		all      []time.Duration
		wg       sync.WaitGroup
		deadline = time.Now().Add(*dur)
	)

	start := time.Now()
	for i := 0; i < *conc; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			local := make([]time.Duration, 0, 4096)
			for time.Now().Before(deadline) {
				u := targets[next.Add(1)%uint64(len(targets))]
				req, err := http.NewRequest(*method, u, strings.NewReader(*body))
				if err != nil {
					failed.Add(1)
					continue
				}
				for _, h := range hdrs {
					if k, v, ok := strings.Cut(h, ":"); ok {
						req.Header.Set(strings.TrimSpace(k), strings.TrimSpace(v))
					}
				}
				t0 := time.Now()
				resp, err := client.Do(req)
				if err != nil {
					failed.Add(1)
					continue
				}
				io.Copy(io.Discard, resp.Body)
				resp.Body.Close()
				local = append(local, time.Since(t0))
				if resp.StatusCode >= 400 {
					failed.Add(1)
				}
			}
			mu.Lock()
			all = append(all, local...)
			mu.Unlock()
		}()
	}
	wg.Wait()
	elapsed := time.Since(start)

	n := len(all)
	fmt.Printf("requests: %d in %s (%.0f req/s), failed: %d\n", n, elapsed.Round(time.Millisecond), float64(n)/elapsed.Seconds(), failed.Load())
	if n == 0 {
		os.Exit(1)
	}
	sort.Slice(all, func(i, j int) bool { return all[i] < all[j] })
	pct := func(p float64) time.Duration { return all[int(float64(n-1)*p)] }
	fmt.Printf("latency:  p50 %s  p95 %s  p99 %s  max %s\n", pct(.50), pct(.95), pct(.99), all[n-1])
	if failed.Load() == uint64(n) {
		os.Exit(1)
	}
}
