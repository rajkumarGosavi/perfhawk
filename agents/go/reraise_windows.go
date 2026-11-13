//go:build windows

package agent

import "os"

// Windows cannot re-raise a signal to itself; exit like an interrupted process.
func reraise(sig os.Signal) {
	os.Exit(130)
}
