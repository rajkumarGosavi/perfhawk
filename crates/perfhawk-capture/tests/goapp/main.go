// Command goapp is a small workload used by perfhawk-capture's gated tests.
// It embeds the perfhawk agent (started when PERFHAWK_AGENT=1).
package main

import (
	"fmt"
	"os"
	"strconv"
	"sync"
	"time"

	_ "github.com/rajkumarGosavi/perfhawk/agents/go"
)

var sink [][]byte

func burn(d time.Duration) {
	end := time.Now().Add(d)
	x := 0
	for time.Now().Before(end) {
		for i := 0; i < 1_000_00; i++ {
			x += i * i
		}
	}
	_ = x
}

func allocate(n int) {
	for i := 0; i < n; i++ {
		sink = append(sink, make([]byte, 1024))
		if len(sink) > 1000 {
			sink = nil
		}
	}
}

func Workload(d time.Duration) {
	var mu sync.Mutex
	var wg sync.WaitGroup
	end := time.Now().Add(d)
	for g := 0; g < 4; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for time.Now().Before(end) {
				mu.Lock()
				burn(2 * time.Millisecond)
				allocate(100)
				mu.Unlock()
				time.Sleep(time.Millisecond)
			}
		}()
	}
	wg.Wait()
}

func main() {
	secs := 30
	if v := os.Getenv("GOAPP_SECONDS"); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			secs = n
		}
	}
	fmt.Println("goapp: running for", secs, "seconds")
	Workload(time.Duration(secs) * time.Second)
}
