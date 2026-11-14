// pocketbase-sample runs an unmodified PocketBase with the perfhawk agent started, so
// `perfhawk record --pid <pid>` can attach. Build and launch it with scripts/samples.
package main

import (
	"log"
	"os"

	"github.com/pocketbase/pocketbase"
	agent "github.com/rajkumarGosavi/perfhawk/agents/go"
)

func main() {
	a, err := agent.Start(agent.Options{})
	if err != nil {
		log.Fatalf("perfhawk agent: %v", err)
	}
	log.Printf("perfhawk agent on %s (pid %d)", a.Addr(), os.Getpid())

	if err := pocketbase.New().Start(); err != nil {
		log.Fatal(err)
	}
}
