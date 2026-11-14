// go-demo is a small "order service" simulation that writes one pprof file per
// profile kind perfhawk understands, so you can try the app without instrumenting
// a real service.
//
//	go run . -out profiles -duration 10s
//
// Output (in -out):
//
//	cpu.pb.gz        CPU time        (json encoding, hashing, sorting)
//	heap.pb.gz       live heap       (a cache that keeps growing)
//	allocs.pb.gz     allocations     (same, but every allocation since start)
//	mutex.pb.gz      lock contention (hot inventory lock)
//	block.pb.gz      blocking waits  (slow channel consumer)
//	goroutine.pb.gz  goroutine dump  (workers parked on channels / locks)
//
// It also serves net/http/pprof on -pprof (default 127.0.0.1:6060) and starts
// the perfhawk agent (-agent, default on), so a live run can be recorded:
//
//	go run . -files=false -duration 60s     # then: perfhawk record --pid <pid>
//
// -files=false skips the demo's own CPU profile, which would otherwise hold
// the runtime's single CPU profiler while perfhawk tries to capture it.
package main

import (
	"crypto/sha256"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"math/rand"
	"net/http"
	_ "net/http/pprof"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"sort"
	"sync"
	"time"

	agent "github.com/rajkumarGosavi/perfhawk/agents/go"
)

type Order struct {
	ID       int       `json:"id"`
	Customer string    `json:"customer"`
	Items    []Item    `json:"items"`
	Created  time.Time `json:"created"`
}

type Item struct {
	SKU   string  `json:"sku"`
	Qty   int     `json:"qty"`
	Price float64 `json:"price"`
}

func main() {
	out := flag.String("out", "profiles", "directory to write profiles into")
	duration := flag.Duration("duration", 10*time.Second, "how long to run the workload")
	pprofAddr := flag.String("pprof", "127.0.0.1:6060", "net/http/pprof listen address (empty to disable)")
	useAgent := flag.Bool("agent", true, "start the perfhawk agent so `perfhawk record --pid` can attach")
	files := flag.Bool("files", true, "write profile files at the end (holds the CPU profiler while running)")
	mode := flag.String("mode", "mixed", "workload: mixed (everything), or cpu, lock, disk (a slow-read stand-in): known-answer runs for `perfhawk doctor`")
	flag.Parse()

	if *files {
		if err := os.MkdirAll(*out, 0o755); err != nil {
			log.Fatal(err)
		}
	}
	if *useAgent {
		a, err := agent.Start(agent.Options{})
		if err != nil {
			log.Fatalf("perfhawk agent: %v", err)
		}
		defer a.Close()
		log.Printf("perfhawk agent on %s (pid %d)", a.Addr(), os.Getpid())
	}

	// Record every mutex contention and every blocking event; sample heap often.
	runtime.SetMutexProfileFraction(1)
	agent.SetBlockProfileRate(1) // like runtime.SetBlockProfileRate, but lets the agent restore it
	runtime.MemProfileRate = 4096

	if *pprofAddr != "" {
		go func() {
			log.Printf("net/http/pprof on http://%s/debug/pprof/", *pprofAddr)
			if err := http.ListenAndServe(*pprofAddr, nil); err != nil {
				log.Printf("pprof server: %v", err)
			}
		}()
	}

	var cpuFile *os.File
	if *files {
		cpuFile = create(*out, "cpu.pb.gz")
		if err := pprof.StartCPUProfile(cpuFile); err != nil {
			log.Fatal(err)
		}
	}

	log.Printf("running workload for %s ...", *duration)
	stop := make(chan struct{})
	var wg sync.WaitGroup
	svc := newService()

	on := func(m string) bool { return *mode == "mixed" || *mode == m }
	if !on("cpu") && !on("lock") && !on("disk") {
		log.Fatalf("unknown -mode %q (want mixed, cpu, lock or disk)", *mode)
	}

	// CPU-heavy request handlers.
	if on("cpu") {
		for i := 0; i < runtime.NumCPU()/2+1; i++ {
			wg.Add(1)
			go func(seed int64) { defer wg.Done(); svc.handleRequests(stop, seed) }(int64(i))
		}
	}
	// Workers fighting over one inventory lock (mutex profile). In lock mode the lock
	// is held ten times longer, so waiting on it dominates.
	if on("lock") {
		hold := 20 * time.Microsecond
		if *mode == "lock" {
			hold = 200 * time.Microsecond
		}
		for i := 0; i < 8; i++ {
			wg.Add(1)
			go func() { defer wg.Done(); svc.reserveInventory(stop, hold) }()
		}
	}
	// Workers blocked in file reads of a slow source: most of their time is I/O wait.
	if *mode == "disk" {
		for i := 0; i < 4; i++ {
			wg.Add(1)
			go func(n int) { defer wg.Done(); svc.diskWorker(stop, n) }(i)
		}
		// Plus real disk traffic, which OS-level tracing (ETW, eBPF) attributes to a file.
		for i := 0; i < 2; i++ {
			wg.Add(1)
			go func(n int) { defer wg.Done(); svc.fileWorker(stop, n) }(i)
		}
	}
	if *mode == "mixed" {
		// Fast producer, slow consumer (block profile).
		events := make(chan Order)
		wg.Add(2)
		go func() { defer wg.Done(); svc.publishEvents(stop, events) }()
		go func() { defer wg.Done(); svc.auditLog(stop, events) }()
		// Idle goroutines parked forever, so the goroutine dump has variety.
		for i := 0; i < 20; i++ {
			go parkedListener(i)
		}
	}

	time.Sleep(*duration)

	if *files {
		// Snapshot goroutines while the workload is still running.
		writeLookup(*out, "goroutine", "goroutine.pb.gz")
	}
	close(stop)
	wg.Wait()
	if !*files {
		log.Printf("done: %d orders cached", len(svc.cache))
		return
	}

	pprof.StopCPUProfile()
	cpuFile.Close()

	writeLookup(*out, "heap", "heap.pb.gz")
	writeLookup(*out, "allocs", "allocs.pb.gz")
	writeLookup(*out, "mutex", "mutex.pb.gz")
	writeLookup(*out, "block", "block.pb.gz")

	abs, _ := filepath.Abs(*out)
	fmt.Printf("\nwrote profiles to %s\nnext:\n  perfhawk import", abs)
	for _, n := range []string{"cpu", "heap", "allocs", "mutex", "block", "goroutine"} {
		fmt.Printf(" %q", filepath.Join(abs, n+".pb.gz"))
	}
	fmt.Println("\n  perfhawk ui")
}

// --- the simulated service -------------------------------------------------

type service struct {
	mu        sync.Mutex
	inventory map[string]int
	cacheMu   sync.Mutex
	cache     map[int][]byte // grows forever: the "leak" the heap profile shows
}

func newService() *service {
	inv := make(map[string]int)
	for i := 0; i < 1000; i++ {
		inv[fmt.Sprintf("sku-%04d", i)] = 1_000_000
	}
	return &service{inventory: inv, cache: make(map[int][]byte)}
}

func (s *service) handleRequests(stop <-chan struct{}, seed int64) {
	rng := rand.New(rand.NewSource(seed))
	for id := int(seed) * 1_000_000; ; id++ {
		select {
		case <-stop:
			return
		default:
		}
		o := randomOrder(rng, id)
		body := encodeOrder(o)
		sum := checksum(body)
		rankItems(o.Items)
		s.remember(id, body, sum)
	}
}

func randomOrder(rng *rand.Rand, id int) Order {
	items := make([]Item, 5+rng.Intn(40))
	for i := range items {
		items[i] = Item{SKU: fmt.Sprintf("sku-%04d", rng.Intn(1000)), Qty: 1 + rng.Intn(5), Price: rng.Float64() * 100}
	}
	return Order{ID: id, Customer: fmt.Sprintf("customer-%d", rng.Intn(10_000)), Items: items, Created: time.Now()}
}

func encodeOrder(o Order) []byte {
	b, err := json.Marshal(o)
	if err != nil {
		panic(err)
	}
	return b
}

func checksum(b []byte) [32]byte {
	sum := sha256.Sum256(b)
	for i := 0; i < 20; i++ { // deliberately wasteful re-hashing
		sum = sha256.Sum256(sum[:])
	}
	return sum
}

func rankItems(items []Item) {
	sort.Slice(items, func(i, j int) bool {
		return items[i].Price*float64(items[i].Qty) > items[j].Price*float64(items[j].Qty)
	})
}

func (s *service) remember(id int, body []byte, sum [32]byte) {
	if id%100 != 0 { // keep the "leak" visible but modest (~tens of MiB)
		return
	}
	entry := make([]byte, len(body)+len(sum))
	copy(entry, body)
	copy(entry[len(body):], sum[:])
	s.cacheMu.Lock()
	s.cache[id] = entry
	s.cacheMu.Unlock()
}

func (s *service) reserveInventory(stop <-chan struct{}, hold time.Duration) {
	rng := rand.New(rand.NewSource(time.Now().UnixNano()))
	for {
		select {
		case <-stop:
			return
		default:
		}
		sku := fmt.Sprintf("sku-%04d", rng.Intn(1000))
		s.mu.Lock()
		s.inventory[sku]--
		spin(hold) // holding the lock while "talking to the DB"
		s.mu.Unlock()
	}
}

// diskWorker stands in for a slow disk: it blocks in os.File reads of a pipe that a
// writer feeds every 10 ms. (A real disk is too fast on a developer machine to be a
// known answer; this spends its time waiting in file reads, not computing.)
func (s *service) diskWorker(stop <-chan struct{}, n int) {
	r, w, err := os.Pipe()
	if err != nil {
		log.Printf("disk worker %d: %v", n, err)
		return
	}
	defer r.Close()
	go func() {
		defer w.Close()
		chunk := make([]byte, 4096)
		for {
			select {
			case <-stop:
				return
			case <-time.After(10 * time.Millisecond):
			}
			if _, err := w.Write(chunk); err != nil {
				return
			}
		}
	}()
	buf := make([]byte, 4096)
	for {
		if _, err := r.Read(buf); err != nil {
			return // the writer closed the pipe: we are stopping
		}
	}
}

// fileWorker does real disk I/O: write 1 MiB to a temp file, fsync it, read it back.
func (s *service) fileWorker(stop <-chan struct{}, n int) {
	f, err := os.CreateTemp("", fmt.Sprintf("perfhawk-demo-%d-*.bin", n))
	if err != nil {
		log.Printf("file worker %d: %v", n, err)
		return
	}
	defer os.Remove(f.Name())
	defer f.Close()
	buf := make([]byte, 1<<20)
	for {
		select {
		case <-stop:
			return
		default:
		}
		if _, err := f.WriteAt(buf, 0); err != nil {
			return
		}
		if err := f.Sync(); err != nil {
			return
		}
		if _, err := f.ReadAt(buf, 0); err != nil {
			return
		}
	}
}

func (s *service) publishEvents(stop <-chan struct{}, events chan<- Order) {
	rng := rand.New(rand.NewSource(42))
	for id := 0; ; id++ {
		select {
		case <-stop:
			close(events)
			return
		case events <- randomOrder(rng, id): // blocks: consumer is slow
		}
	}
}

func (s *service) auditLog(stop <-chan struct{}, events <-chan Order) {
	for o := range events {
		_ = o
		time.Sleep(2 * time.Millisecond) // pretend to write to a slow disk
	}
}

func parkedListener(i int) {
	select {} // blocks forever; shows up in the goroutine profile
}

func spin(d time.Duration) {
	end := time.Now().Add(d)
	for time.Now().Before(end) {
	}
}

// --- helpers -------------------------------------------------------------

func create(dir, name string) *os.File {
	f, err := os.Create(filepath.Join(dir, name))
	if err != nil {
		log.Fatal(err)
	}
	return f
}

func writeLookup(dir, profile, name string) {
	f := create(dir, name)
	defer f.Close()
	if err := pprof.Lookup(profile).WriteTo(f, 0); err != nil {
		log.Fatal(err)
	}
	log.Printf("wrote %s", name)
}
