module perfhawk/examples/go-demo

go 1.22

require github.com/rajkumarGosavi/perfhawk/agents/go v0.0.0

require (
	github.com/felixge/fgprof v0.9.5 // indirect
	github.com/google/pprof v0.0.0-20240227163752-401108e1b7e7 // indirect
)

replace github.com/rajkumarGosavi/perfhawk/agents/go => ../../agents/go
