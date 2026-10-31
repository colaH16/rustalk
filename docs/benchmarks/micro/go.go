package main

import (
	"encoding/json"
	"flag"
	"net/url"
	"os"
	"runtime"
	"time"
)

type Message struct {
	ID          int    `json:"id"`
	Content     string `json:"content"`
	SenderPod   string `json:"sender_pod"`
	SenderNick  string `json:"sender_nick"`
	SenderColor string `json:"sender_color"`
	Time        string `json:"time"`
}

var bytesSink []byte
var fieldsSink url.Values

func main() {
	kind := flag.String("case", "json", "")
	n := flag.Int("n", 500000, "")
	flag.Parse()
	m := Message{42, "hello Rustalk", "bench-pod", "bench-user", "#abcdef", "12:34:56"}
	input := "nick=bench-user&color=%23abcdef&msg=hello+Rustalk%21"
	op := func() {
		if *kind == "json" {
			bytesSink, _ = json.Marshal(m)
		} else {
			fieldsSink, _ = url.ParseQuery(input)
		}
	}
	for i := 0; i < 10000; i++ {
		op()
	}
	runtime.GC()
	var a, b runtime.MemStats
	runtime.ReadMemStats(&a)
	start := time.Now()
	for i := 0; i < *n; i++ {
		op()
	}
	elapsed := time.Since(start)
	runtime.ReadMemStats(&b)
	json.NewEncoder(os.Stdout).Encode(map[string]any{"case": *kind, "iterations": *n, "ns_per_op": float64(elapsed.Nanoseconds()) / float64(*n), "allocs_per_op": float64(b.Mallocs-a.Mallocs) / float64(*n), "bytes_per_op": float64(b.TotalAlloc-a.TotalAlloc) / float64(*n), "gc_cycles": b.NumGC - a.NumGC, "gc_pause_total_us": float64(b.PauseTotalNs-a.PauseTotalNs) / 1000, "sample_json": string(bytesSink), "sample_fields": fieldsSink})
}
