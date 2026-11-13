// Generates the Go pprof fixtures used by perfhawk-import tests.
// Usage: go run . <out-dir>
package main

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"sync"
	"time"
)

var sink [][]byte

//go:noinline
func burn(d time.Duration) {
	end := time.Now().Add(d)
	x := 0
	for time.Now().Before(end) {
		for i := 0; i < 100000; i++ {
			x += i * i
		}
	}
	if x == 42 {
		fmt.Println(x)
	}
}

func alloc() {
	for i := 0; i < 2000; i++ {
		sink = append(sink, make([]byte, 4096))
	}
}

func contend() {
	var mu sync.Mutex
	var wg sync.WaitGroup
	for g := 0; g < 4; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := 0; i < 50; i++ {
				mu.Lock()
				time.Sleep(time.Millisecond)
				mu.Unlock()
			}
		}()
	}
	wg.Wait()
}

func write(dir, name string, p *pprof.Profile) {
	f, err := os.Create(filepath.Join(dir, name))
	if err != nil {
		panic(err)
	}
	defer f.Close()
	if err := p.WriteTo(f, 0); err != nil {
		panic(err)
	}
}

func main() {
	dir := os.Args[1]
	runtime.MemProfileRate = 1
	runtime.SetMutexProfileFraction(1)

	cf, err := os.Create(filepath.Join(dir, "go-cpu.pb.gz"))
	if err != nil {
		panic(err)
	}
	if err := pprof.StartCPUProfile(cf); err != nil {
		panic(err)
	}
	burn(2500 * time.Millisecond)
	pprof.StopCPUProfile()
	cf.Close()

	alloc()
	contend()
	runtime.GC()
	write(dir, "go-heap.pb.gz", pprof.Lookup("heap"))
	write(dir, "go-mutex.pb.gz", pprof.Lookup("mutex"))
}
