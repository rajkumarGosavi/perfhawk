package main

import (
	"testing"
	"time"
)

func TestWorkload(t *testing.T) {
	Workload(1500 * time.Millisecond)
}
