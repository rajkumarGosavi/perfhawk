//go:build !windows

package agent

import (
	"os"
	"syscall"
)

func reraise(sig os.Signal) {
	if s, ok := sig.(syscall.Signal); ok {
		_ = syscall.Kill(os.Getpid(), s)
		return
	}
	os.Exit(130)
}
