# Benchmarks

`cym-bench` times the core on a generated developer home folder and on synthetic result rows:

~~~
cargo run --release --bin cym-bench            # 1M rows, 5 runs, median
cargo run --release --bin cym-bench -- --rows 200000 --runs 3
~~~

The fixture (`.bench-fixture/`, git-ignored, about 122,000 entries) is generated once:
- 200 JavaScript projects with installed dependencies, half of them Next.js apps;
- 100 Rust crates with build output;
- 100 Python projects with virtual environments;
- 300 document trees;
- 500 sets of four identical 8 KB files, plus 3,000 unique files of the same size;
- five large sparse videos.

Scans run warm (one discarded run, then the median), so they measure the core's own work rather than the disk. The result store uses `results::synthetic`. A run that exceeds 60 seconds is reported as such instead of hanging.

## October 2026 optimization pass

Linux container, 4 threads, warm cache. Medians of five runs (three for store inserts and first queries).

| Benchmark | Before | After | Change |
| --- | ---: | ---: | --- |
| Walk (scope checks only), 152,716 entries | 612 ms | 444 ms | 1.4× |
| Scan · Large Files | 672 ms | 454 ms | 1.5× |
| Scan · Exact Duplicates | 201 ms | 167 ms | 1.2× |
| Scan · Node Dependencies | 276 ms | 267 ms | — |
| Scan · Build Artifacts | 173 ms | 141 ms | 1.2× |
| Scan · Storage Explorer | 136 ms | 113 ms | 1.2× |
| Store · insert, 1M rows | 910 ms | 500 ms | 1.8× |
| Store · first query by size, 1M rows | 1,689 ms | 433 ms | 3.9× |
| Store · repeat query, 1M rows | 117 ms | 2 µs | ~58,000× |
| Store · first query by name, 1M rows | 1,738 ms | 787 ms | 2.2× |
| Store · search, 1M rows | 154 ms | 115 ms | 1.3× |
| Store · size filter, 1M rows | 76.5 ms | 9.5 ms | 8× |
| Store · page of 200 | 43 µs | 39 µs | — |
| Store · select all (941,176 eligible IDs) | 202 ms | 192 ms | — |
| Store · selection summary, 1k selected | 161 ms | 1.4 ms | 115× |
| Store · selection summary, 10k selected | 15,950 ms | 16.5 ms | 970× |
| Store · selection summary, 100k selected | > 60 s | 261 ms | > 230× |

What changed:

- **Selection was quadratic.** Dropping items inside another selected folder compared every selected pair. That took 16 s for 10,000 items, and selecting 100,000 (one click with Select All) did not finish, in the summary and again when reviewing and applying. `policy::outermost` now looks each item's parent folders up in a set of selected paths, at O(n × depth). The summary works on shared rows instead of deep copies, and finds each ID's module bucket from its `module:` prefix.
- **Totals took 1.7 s of every first query.** `analytics_of` allocated a vector per path and rehashed every parent of every path. It also made four extra single-threaded passes over the rows. It now:
  - hashes paths byte by byte, so all of a path's parents are found in one pass;
  - chains rows that share a path by index;
  - de-duplicates modules with a bitmask;
  - counts rows in one parallel pass.

  If two different paths ever share a hash, it falls back to the original algorithm (`analytics_reference`).
- **Repeat queries copied a million rows.** An unfiltered snapshot now shares the cached sorted rows. The totals, the five largest items, the highest CPU and the eligible count are cached per filter, and computed together in one pass when needed.
- **Sorting** compares precomputed integer keys instead of following two pointers per comparison. IDs break ties exactly as before.
- **Insert** reserves each module's capacity once and wraps rows in parallel.
- **Walking** with read-ahead was slower than plain traversal when folders came from the cache. A cached folder lists in microseconds, less than the cost of handing it to another thread. `PrefetchWalker` now times its listings and reads ahead only while they average over 50 µs, which is when it waits on the disk.

Each change has an equivalence test against the previous implementation:
- `outermost_selection_matches_the_pairwise_reference`
- `fast_analytics_matches_the_reference_totals`
- `keyed_sorting_matches_the_reference_comparator`
- `prefetching_walker_visits_in_stack_order_and_honors_skip_and_stop`
