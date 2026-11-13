// go-demo is a small "order service" simulation that writes one pprof file per
// profile kind prism understands, so you can try the app without instrumenting
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
// the prism agent (-agent, default on), so a live run can be recorded:
//
//	go run . -files=false -duration 60s     # then: prism record --pid <pid>
//
// -files=false skips the demo's own CPU profile, which would otherwise hold
// the runtime's single CPU profiler while prism tries to capture it.
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

	agent "github.com/rajkumarGosavi/prism/agents/go"
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
	useAgent := flag.Bool("agent", true, "start the prism agent so `prism record --pid` can attach")
	files := flag.Bool("files", true, "write profile files at the end (holds the CPU profiler while running)")
	flag.Parse()

	if *files {
		if err := os.MkdirAll(*out, 0o755); err != nil {
			log.Fatal(err)
		}
	}
	if *useAgent {
		a, err := agent.Start(agent.Options{})
		if err != nil {
			log.Fatalf("prism agent: %v", err)
		}
		defer a.Close()
		log.Printf("prism agent on %s (pid %d)", a.Addr(), os.Getpid())
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

	// CPU-heavy request handlers.
	for i := 0; i < runtime.NumCPU()/2+1; i++ {
		wg.Add(1)
		go func(seed int64) { defer wg.Done(); svc.handleRequests(stop, seed) }(int64(i))
	}
	// Workers fighting over one inventory lock (mutex profile).
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); svc.reserveInventory(stop) }()
	}
	// Fast producer, slow consumer (block profile).
	events := make(chan Order)
	wg.Add(2)
	go func() { defer wg.Done(); svc.publishEvents(stop, events) }()
	go func() { defer wg.Done(); svc.auditLog(stop, events) }()
	// Idle goroutines parked forever, so the goroutine dump has variety.
	for i := 0; i < 20; i++ {
		go parkedListener(i)
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
	fmt.Printf("\nwrote profiles to %s\nnext:\n  prism import", abs)
	for _, n := range []string{"cpu", "heap", "allocs", "mutex", "block", "goroutine"} {
		fmt.Printf(" %q", filepath.Join(abs, n+".pb.gz"))
	}
	fmt.Println("\n  prism ui")
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

func (s *service) reserveInventory(stop <-chan struct{}) {
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
		spin(20 * time.Microsecond) // holding the lock while "talking to the DB"
		s.mu.Unlock()
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
