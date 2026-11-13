package agent

import (
	"encoding/json"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"syscall"
)

func discoveryDir() string { return filepath.Join(os.TempDir(), "perfhawk-agents") }

func (a *Agent) writeDiscovery() error {
	if err := os.MkdirAll(discoveryDir(), 0o700); err != nil {
		return err
	}
	a.discovery = filepath.Join(discoveryDir(), fmt.Sprintf("%d.json", os.Getpid()))
	body, err := json.Marshal(map[string]any{
		"pid":        os.Getpid(),
		"addr":       a.Addr(),
		"go_version": runtime.Version(),
		"protocol":   Protocol,
	})
	if err != nil {
		return err
	}
	return os.WriteFile(a.discovery, body, 0o600)
}

func (a *Agent) removeDiscovery() {
	if a.discovery != "" {
		_ = os.Remove(a.discovery)
	}
}

// watchSignals removes the discovery file on SIGINT/SIGTERM, but only when
// PERFHAWK_AGENT_SIGNALS=1 so the agent never takes over the application's own
// signal handling by default. After cleaning up it restores default handling
// and re-raises the signal so the process still terminates.
func watchSignals(a *Agent) {
	if os.Getenv("PERFHAWK_AGENT_SIGNALS") != "1" {
		return
	}
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt, syscall.SIGTERM)
	go func() {
		sig := <-ch
		a.removeDiscovery()
		signal.Stop(ch)
		reraise(sig)
	}()
}
