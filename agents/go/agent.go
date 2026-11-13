// Package agent embeds a small perfhawk profiling endpoint in a Go program.
//
// It serves the standard net/http/pprof handlers plus wall-clock profiling
// (fgprof) and temporary mutex/block profiling, on a loopback-only address,
// and publishes a discovery file so `perfhawk record --pid` can find it.
//
// Importing the package with a blank import and setting PERFHAWK_AGENT=1 starts
// the agent automatically; otherwise call Start.
package agent

import (
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/http/pprof"
	"os"
	"runtime"
	"sync"
	"time"

	"github.com/felixge/fgprof"
)

// Protocol is the version of the HTTP contract perfhawk speaks to this agent.
const Protocol = 1

// Options configures Start.
type Options struct {
	// Addr is the listen address; default "127.0.0.1:0". Must be loopback.
	Addr string
	// DisableDiscovery skips writing the discovery file.
	DisableDiscovery bool
}

// Agent is a running perfhawk endpoint.
type Agent struct {
	srv       *http.Server
	ln        net.Listener
	discovery string
	closeOnce sync.Once
	closeErr  error
}

func init() {
	if os.Getenv("PERFHAWK_AGENT") == "1" {
		if _, err := Start(Options{}); err != nil {
			fmt.Fprintf(os.Stderr, "perfhawk agent: %v\n", err)
		}
	}
}

func checkLoopback(addr string) error {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return fmt.Errorf("invalid address %q: %w", addr, err)
	}
	if host == "localhost" {
		return nil
	}
	ip := net.ParseIP(host)
	if ip == nil || !ip.IsLoopback() {
		return fmt.Errorf("address %q is not loopback; the perfhawk agent only listens on loopback", addr)
	}
	return nil
}

// Start launches the agent's HTTP server.
func Start(opts Options) (*Agent, error) {
	addr := opts.Addr
	if addr == "" {
		addr = "127.0.0.1:0"
	}
	if err := checkLoopback(addr); err != nil {
		return nil, err
	}
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return nil, err
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/debug/pprof/", pprof.Index)
	mux.HandleFunc("/debug/pprof/cmdline", pprof.Cmdline)
	mux.HandleFunc("/debug/pprof/profile", pprof.Profile)
	mux.HandleFunc("/debug/pprof/symbol", pprof.Symbol)
	mux.HandleFunc("/debug/pprof/trace", pprof.Trace)
	mux.HandleFunc("/debug/perfhawk/info", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"protocol":   Protocol,
			"pid":        os.Getpid(),
			"go_version": runtime.Version(),
		})
	})
	mux.Handle("/debug/perfhawk/wall", fgprof.Handler())
	mux.HandleFunc("/debug/perfhawk/contention", contentionHandler)
	mux.HandleFunc("/debug/perfhawk/cpu-windows", cpuWindowsHandler)

	a := &Agent{
		srv: &http.Server{Handler: mux, ReadHeaderTimeout: 10 * time.Second},
		ln:  ln,
	}
	go func() { _ = a.srv.Serve(ln) }()
	if !opts.DisableDiscovery {
		// The discovery directory is shared (the temp dir); failing to write there, for
		// example because another user owns it, must not keep the agent from running.
		if err := a.writeDiscovery(); err != nil {
			fmt.Fprintf(os.Stderr, "perfhawk agent: discovery file not written (use --url to attach): %v\n", err)
		} else {
			watchSignals(a)
		}
	}
	return a, nil
}

// Addr is the actual listen address, e.g. "127.0.0.1:41234".
func (a *Agent) Addr() string { return a.ln.Addr().String() }

// Close stops the server and removes the discovery file.
func (a *Agent) Close() error {
	a.closeOnce.Do(func() {
		a.removeDiscovery()
		a.closeErr = a.srv.Close()
	})
	return a.closeErr
}
