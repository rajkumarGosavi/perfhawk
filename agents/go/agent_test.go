package agent

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"sync"
	"testing"
	"time"

	"github.com/google/pprof/profile"
)

func get(t *testing.T, url string) []byte {
	t.Helper()
	resp, err := http.Get(url)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != 200 {
		t.Fatalf("GET %s: %d %s", url, resp.StatusCode, b)
	}
	return b
}

func start(t *testing.T, opts Options) *Agent {
	t.Helper()
	a, err := Start(opts)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = a.Close() })
	return a
}

func discoveryFile() string {
	return filepath.Join(os.TempDir(), "prism-agents", fmt.Sprintf("%d.json", os.Getpid()))
}

func TestStartWritesDiscoveryAndCloseRemoves(t *testing.T) {
	a, err := Start(Options{})
	if err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(discoveryFile())
	if err != nil {
		t.Fatalf("discovery file missing: %v", err)
	}
	var d struct {
		Pid       int    `json:"pid"`
		Addr      string `json:"addr"`
		GoVersion string `json:"go_version"`
		Protocol  int    `json:"protocol"`
	}
	if err := json.Unmarshal(raw, &d); err != nil {
		t.Fatal(err)
	}
	if d.Pid != os.Getpid() || d.Addr != a.Addr() || d.Protocol != 1 || d.GoVersion != runtime.Version() {
		t.Fatalf("bad discovery content: %s (addr %s)", raw, a.Addr())
	}
	if runtime.GOOS != "windows" {
		st, _ := os.Stat(discoveryFile())
		if st.Mode().Perm() != 0o600 {
			t.Fatalf("discovery perms %v", st.Mode().Perm())
		}
	}
	if err := a.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(discoveryFile()); !os.IsNotExist(err) {
		t.Fatalf("discovery file not removed: %v", err)
	}
}

func TestDisableDiscovery(t *testing.T) {
	_ = os.Remove(discoveryFile())
	start(t, Options{DisableDiscovery: true})
	if _, err := os.Stat(discoveryFile()); !os.IsNotExist(err) {
		t.Fatal("discovery file written despite DisableDiscovery")
	}
}

func TestRejectsNonLoopbackAddr(t *testing.T) {
	for _, addr := range []string{"0.0.0.0:0", ":0", "192.168.1.5:0", "example.com:80"} {
		if a, err := Start(Options{Addr: addr, DisableDiscovery: true}); err == nil {
			_ = a.Close()
			t.Fatalf("expected error for %q", addr)
		}
	}
}

func TestInfoEndpoint(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	var info struct {
		Protocol  int    `json:"protocol"`
		Pid       int    `json:"pid"`
		GoVersion string `json:"go_version"`
	}
	if err := json.Unmarshal(get(t, "http://"+a.Addr()+"/debug/prism/info"), &info); err != nil {
		t.Fatal(err)
	}
	if info.Protocol != 1 || info.Pid != os.Getpid() || info.GoVersion != runtime.Version() {
		t.Fatalf("bad info %+v", info)
	}
	// standard pprof index is served too
	if !bytes.Contains(get(t, "http://"+a.Addr()+"/debug/pprof/"), []byte("goroutine")) {
		t.Fatal("pprof index missing")
	}
}

func TestWallReturnsPprof(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	body := get(t, "http://"+a.Addr()+"/debug/prism/wall?seconds=1")
	p, err := profile.Parse(bytes.NewReader(body))
	if err != nil {
		t.Fatalf("not a pprof profile: %v", err)
	}
	if len(p.SampleType) == 0 {
		t.Fatal("no sample types")
	}
}

// contend runs goroutines fighting over a mutex until the returned stop func is called.
func contend() (stop func()) {
	var mu sync.Mutex
	done := make(chan struct{})
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				select {
				case <-done:
					return
				default:
				}
				mu.Lock()
				time.Sleep(time.Millisecond)
				mu.Unlock()
			}
		}()
	}
	return func() { close(done); wg.Wait() }
}

func TestContentionRestoresRates(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	runtime.SetMutexProfileFraction(0)
	// Start contending after the capture has raised the rate: waits that began
	// while profiling was off are never attributed.
	var stop func()
	timer := time.AfterFunc(300*time.Millisecond, func() { stop = contend() })
	defer func() {
		timer.Stop()
		time.Sleep(50 * time.Millisecond)
		if stop != nil {
			stop()
		}
	}()
	body := get(t, "http://"+a.Addr()+"/debug/prism/contention?seconds=2&kind=mutex")
	p, err := profile.Parse(bytes.NewReader(body))
	if err != nil {
		t.Fatalf("not a pprof profile: %v", err)
	}
	if len(p.Sample) < 1 {
		t.Fatal("expected at least one mutex contention sample")
	}
	if prev := runtime.SetMutexProfileFraction(-1); prev != 0 {
		t.Fatalf("mutex fraction not restored: %d", prev)
	}
}

func TestContentionBlockRestoresRate(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	SetBlockProfileRate(0)
	stop := contend()
	defer stop()
	body := get(t, "http://"+a.Addr()+"/debug/prism/contention?seconds=1&kind=block")
	if _, err := profile.Parse(bytes.NewReader(body)); err != nil {
		t.Fatalf("not a pprof profile: %v", err)
	}
	if got := blockRate.Load(); got != 0 {
		t.Fatalf("tracked block rate %d", got)
	}
	// An app-chosen rate set through the wrapper is what gets restored.
	SetBlockProfileRate(1)
	_ = get(t, "http://"+a.Addr()+"/debug/prism/contention?seconds=1&kind=block")
	if got := blockRate.Load(); got != 1 {
		t.Fatalf("app block rate not preserved: %d", got)
	}
	SetBlockProfileRate(0)
}

func TestContentionBadKind(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	resp, err := http.Get("http://" + a.Addr() + "/debug/prism/contention?seconds=1&kind=nope")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != 400 {
		t.Fatalf("status %d", resp.StatusCode)
	}
}

func TestContentionCancelledRequestRestores(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	runtime.SetMutexProfileFraction(0)
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, "GET", "http://"+a.Addr()+"/debug/prism/contention?seconds=30&kind=mutex", nil)
	if _, err := http.DefaultClient.Do(req); err == nil {
		t.Fatal("expected client timeout")
	}
	deadline := time.Now().Add(3 * time.Second)
	for {
		if prev := runtime.SetMutexProfileFraction(-1); prev == 0 {
			return
		}
		if time.Now().After(deadline) {
			t.Fatal("mutex fraction not restored after cancelled request")
		}
		time.Sleep(50 * time.Millisecond)
	}
}

func TestHelperProcess(t *testing.T) {
	if os.Getenv("PRISM_AGENT_HELPER") != "1" {
		t.Skip("helper process only")
	}
	// Block until stdin closes so the parent controls our lifetime.
	_, _ = io.Copy(io.Discard, os.Stdin)
}

func runHelper(t *testing.T, env ...string) (discovery string, stop func()) {
	t.Helper()
	tmp := t.TempDir()
	cmd := exec.Command(os.Args[0], "-test.run=TestHelperProcess")
	cmd.Env = append(os.Environ(), "PRISM_AGENT_HELPER=1", "TMP="+tmp, "TEMP="+tmp, "TMPDIR="+tmp)
	cmd.Env = append(cmd.Env, env...)
	in, err := cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	stop = func() { _ = in.Close(); _ = cmd.Wait() }
	return filepath.Join(tmp, "prism-agents", fmt.Sprintf("%d.json", cmd.Process.Pid)), stop
}

func TestAutoStartOnlyWithEnv(t *testing.T) {
	file, stop := runHelper(t, "PRISM_AGENT=1")
	defer stop()
	deadline := time.Now().Add(10 * time.Second)
	for {
		if _, err := os.Stat(file); err == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("discovery file did not appear with PRISM_AGENT=1")
		}
		time.Sleep(50 * time.Millisecond)
	}

	file2, stop2 := runHelper(t, "PRISM_AGENT=")
	defer stop2()
	time.Sleep(1500 * time.Millisecond)
	if _, err := os.Stat(file2); !os.IsNotExist(err) {
		t.Fatal("agent started without PRISM_AGENT=1")
	}
}

func TestStartSucceedsWhenDiscoveryCannotBeWritten(t *testing.T) {
	// Another user may own the shared discovery directory; that must not stop the agent.
	blocker := filepath.Join(t.TempDir(), "not-a-dir")
	if err := os.WriteFile(blocker, []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	for _, k := range []string{"TMP", "TEMP", "TMPDIR"} {
		t.Setenv(k, blocker)
	}
	a, err := Start(Options{})
	if err != nil {
		t.Fatalf("Start failed because discovery was unwritable: %v", err)
	}
	defer a.Close()
	get(t, "http://"+a.Addr()+"/debug/prism/info")
}

func busyLoop(stop <-chan struct{}) {
	go func() {
		x := 0
		for {
			select {
			case <-stop:
				return
			default:
				for i := 0; i < 1_000_000; i++ {
					x += i * i
				}
				if x == -1 {
					fmt.Println(x)
				}
			}
		}
	}()
}

func TestCPUWindowsReturnsLabelledWindows(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	stop := make(chan struct{})
	defer close(stop)
	busyLoop(stop)
	body := get(t, "http://"+a.Addr()+"/debug/prism/cpu-windows?seconds=1&window_ms=100")
	p, err := profile.ParseData(body)
	if err != nil {
		t.Fatalf("not a pprof: %v", err)
	}
	if len(p.Sample) == 0 {
		t.Fatal("no samples")
	}
	seen := map[int64]bool{}
	for _, s := range p.Sample {
		v, ok := s.NumLabel["prism.bucket"]
		if !ok || len(v) != 1 {
			t.Fatalf("sample without prism.bucket label: %+v", s.NumLabel)
		}
		seen[v[0]] = true
	}
	if len(seen) < 5 {
		t.Fatalf("want at least 5 distinct windows in 1 s at 100 ms, got %d: %v", len(seen), seen)
	}
	for b := range seen {
		if b < 0 || b > 20 {
			t.Fatalf("bucket %d out of range", b)
		}
	}
}

func TestCPUWindowsBadParams(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	for _, q := range []string{"", "?seconds=0", "?seconds=601", "?seconds=x", "?seconds=1&window_ms=10", "?seconds=1&window_ms=99999"} {
		resp, err := http.Get("http://" + a.Addr() + "/debug/prism/cpu-windows" + q)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		if resp.StatusCode != 400 {
			t.Fatalf("%q: want 400, got %d", q, resp.StatusCode)
		}
	}
}

func TestCPUWindowsConflictsWithRunningProfile(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	var sink bytes.Buffer
	if err := pprof.StartCPUProfile(&sink); err != nil {
		t.Skipf("cannot start a CPU profile here: %v", err)
	}
	defer pprof.StopCPUProfile()
	resp, err := http.Get("http://" + a.Addr() + "/debug/prism/cpu-windows?seconds=1")
	if err != nil {
		t.Fatal(err)
	}
	resp.Body.Close()
	if resp.StatusCode != 409 {
		t.Fatalf("want 409 while another CPU profile runs, got %d", resp.StatusCode)
	}
}

func TestCPUWindowsCancelledRequestFreesTheProfiler(t *testing.T) {
	a := start(t, Options{DisableDiscovery: true})
	ctx, cancel := context.WithTimeout(context.Background(), 400*time.Millisecond)
	defer cancel()
	req, _ := http.NewRequestWithContext(ctx, "GET", "http://"+a.Addr()+"/debug/prism/cpu-windows?seconds=30&window_ms=100", nil)
	if resp, err := http.DefaultClient.Do(req); err == nil {
		resp.Body.Close()
	}
	// The profiler must be usable again shortly after the connection drops.
	deadline := time.Now().Add(3 * time.Second)
	for {
		var sink bytes.Buffer
		if err := pprof.StartCPUProfile(&sink); err == nil {
			pprof.StopCPUProfile()
			return
		}
		if time.Now().After(deadline) {
			t.Fatal("CPU profiler still busy after the request was cancelled")
		}
		time.Sleep(50 * time.Millisecond)
	}
}

func TestRejectsNonLoopbackHost(t *testing.T) {
	a, err := Start(Options{DisableDiscovery: true})
	if err != nil {
		t.Fatal(err)
	}
	defer a.Close()
	for host, want := range map[string]int{
		a.Addr():          200,
		"localhost:1234":  200,
		"[::1]:80":        200,
		"evil.example":    403,
		"evil.example:80": 403,
		"127.0.0.1.evil":  403,
	} {
		req, _ := http.NewRequest("GET", "http://"+a.Addr()+"/debug/prism/info", nil)
		req.Host = host
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		resp.Body.Close()
		if resp.StatusCode != want {
			t.Errorf("Host %q: got %d want %d", host, resp.StatusCode, want)
		}
	}
}
