package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"sort"
	"strings"
	"sync"
	"time"
)

func main() {
	url := flag.String("url", "", "")
	n := flag.Int("n", 2000, "")
	c := flag.Int("c", 1, "")
	body := flag.String("body", "", "")
	flag.Parse()
	transport := &http.Transport{MaxIdleConnsPerHost: *c, MaxConnsPerHost: *c, DisableCompression: true}
	client := &http.Client{Transport: transport, Timeout: 10 * time.Second}
	defer transport.CloseIdleConnections()
	values := make([]int64, *n)
	errors := 0
	var mu sync.Mutex
	var wg sync.WaitGroup
	start := time.Now()
	for worker := 0; worker < *c; worker++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for i := w; i < *n; i += *c {
				method := "GET"
				if *body != "" {
					method = "POST"
				}
				r, _ := http.NewRequest(method, *url, strings.NewReader(*body))
				if *body != "" {
					r.Header.Set("Content-Type", "application/x-www-form-urlencoded")
				}
				t := time.Now()
				resp, e := client.Do(r)
				ok := e == nil
				if ok {
					_, e = io.Copy(io.Discard, resp.Body)
					resp.Body.Close()
					ok = e == nil && resp.StatusCode == 200
				}
				values[i] = time.Since(t).Nanoseconds()
				if !ok {
					mu.Lock()
					errors++
					mu.Unlock()
				}
			}
		}(worker)
	}
	wg.Wait()
	elapsed := time.Since(start)
	sort.Slice(values, func(i, j int) bool { return values[i] < values[j] })
	var sum int64
	for _, v := range values {
		sum += v
	}
	q := func(p float64) int64 { return values[int(float64(len(values)-1)*p)] }
	json.NewEncoder(os.Stdout).Encode(map[string]any{"requests": *n, "concurrency": *c, "errors": errors, "elapsed_s": elapsed.Seconds(), "rps": float64(*n) / elapsed.Seconds(), "mean_us": float64(sum) / float64(*n) / 1000, "p50_us": float64(q(.5)) / 1000, "p95_us": float64(q(.95)) / 1000, "p99_us": float64(q(.99)) / 1000})
	if errors > 0 {
		fmt.Fprintln(os.Stderr, "request errors")
		os.Exit(1)
	}
}
