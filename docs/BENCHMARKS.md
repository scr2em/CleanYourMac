# Benchmarks

`cym-bench` times the core on a generated developer home folder and on synthetic result rows:

~~~
cargo run --release --bin cym-bench            # 1M rows, 5 runs, median
cargo run --release --bin cym-bench -- --rows 200000 --runs 3
cargo run --release --bin cym-bench -- --walkers-only            # walker comparison, warm
sudo cargo run --release --bin cym-bench -- --walkers-only --cold # also with the cache dropped
~~~

`--cold` drops the file system cache before every run (`purge` on macOS, `/proc/sys/vm/drop_caches` on Linux), so it needs root. `CYM_WALK_THREADS=N` sets the parallel walker's listing threads.

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

## Parallel walker

`ParallelWalker` replaced `PrefetchWalker` as the default. Visiting stays on the calling thread in the same depth-first order, so no scan module changed. What runs in parallel is the folder listing: while listings wait on the disk, a few threads (cores minus one, at most seven) list the folders already queued, nearest the top of the stack first. The calling thread lists the folder it needs next itself, unless a listing thread is already reading it. Skipped folders are never queued, so no listing is wasted, and at most 200,000 entries are listed ahead of the walk.

Linux VM, 4 vCPUs, 3 listing threads. Same fixture, 152,716 entries. Medians of five runs. Cold runs drop the page cache before every run.

| Walk | Stack | Read-ahead (before) | Parallel | Parallel vs stack |
| --- | ---: | ---: | ---: | --- |
| Listing only, warm | 523 ms | 523 ms | 506 ms | — |
| With scope checks, warm | 618 ms | 612 ms | 576 ms | — |
| Large Files scan, warm | 628 ms | 664 ms | 647 ms | — |
| Listing only, cold | 4,252 ms | 3,402 ms | 2,933 ms | 1.45× |
| With scope checks, cold | 4,175 ms | 3,883 ms | 3,280 ms | 1.27× |
| Large Files scan, cold | 4,207 ms | 3,725 ms | 3,175 ms | 1.33× |

Warm differences are within run-to-run noise. Listing threads on a cold cache, listing only (three runs each):

| Listing threads | 1 | 3 | 7 | 15 |
| --- | ---: | ---: | ---: | ---: |
| Cold listing | 3,765 ms | 2,701 ms | 3,269 ms | 3,528 ms |

What we learned:

- **Parallel listing with a warm cache was 1.8× slower** than the stack (1,013 ms against 556 ms) in the first version. A cached folder lists in microseconds, less than waking another thread costs. Like `PrefetchWalker`, the walker now times its own listings and offers folders to other threads only while they average over 50 µs. Warm walks then match the stack, and the threads never start.
- **More threads than cores did not help** in this VM. Listing a cold folder also costs kernel CPU time, so the 4 vCPUs were the limit beyond three listing threads. On a Mac with an NVMe disk and more cores the best count may differ: compare with `CYM_WALK_THREADS` and `--cold`.
- **The rest of a cold walk is order-bound.** The visitor decides whether to enter each folder, so only folders it already chose can be listed ahead. Listing deeper speculatively would also list folders the scan skips, such as `node_modules`.

Equivalence: `prefetching_walker_visits_in_stack_order_and_honors_skip_and_stop` compares the visit order with `StackWalker` for 0, 1 and 3 listing threads, with stopping early and with a read-ahead limit small enough that the threads pause and resume. The same comparison on the 152,716-entry fixture, with 1 to 15 threads and parallel listing forced on, matched in every run.

## Against npkill

[npkill](https://github.com/zaldih/npkill) 0.12.2 finds `node_modules` folders with Node worker threads, then sizes each one by spawning `du -sk | cut` (two at a time) and reads its parent folder for the newest change. `scripts/npkill-compare/` runs that same pipeline without the terminal UI, and times both tools as whole processes on the same folder:

~~~
cargo build --release --bin cym
npm install --prefix scripts/npkill-compare npkill@0.12.2
sudo python3 scripts/npkill-compare/compare.py ~/some/folder 5   # without sudo: warm only
~~~

Linux VM, 4 vCPUs. The benchmark fixture copied to a non-hidden path (152,716 entries, 200 `node_modules`), because npkill skips its last-modified step on hidden paths. Medians of five runs; cold runs drop the page cache before every run.

| | CleanYourMac `cym scan node` | npkill | Faster |
| --- | ---: | ---: | --- |
| Warm | 352 ms | 3,401 ms | 9.7× |
| Cold | 1,893 ms | 4,451 ms | 2.4× |

- **Same folders found:** both report the same 200 `node_modules`.
- **npkill spends most of its time sizing.** Its search finishes in about 1 s; spawning `du` for each folder, two at a time, takes the next 2 to 3 s. CleanYourMac sizes folders in-process and in parallel.
- **CleanYourMac does more in that time.** The Dependencies scan also reports Python environments and other ecosystems (303 findings in all), with project, package manager and last-used details.
- **Size totals differ by design.** npkill's `du` also counts the blocks of folders themselves: 4 KB per folder on ext4, 504 KB per `node_modules` here, 100,800 KB in all. CleanYourMac counts the blocks of files, which agree exactly with `du`. On APFS folders take almost no blocks, so on a Mac the totals should nearly match.
- **This is the Linux harness.** On macOS npkill also uses `du`, and CleanYourMac lists folders with `getattrlistbulk`. Run the script on a Mac to confirm the numbers there.

## Profiling the walk

The question was whether rewriting part of the core in C would make scans faster. `callgrind` (valgrind) on `cym scan` showed it would not. Warm scans spend 45–65% of their CPU time in the macOS kernel (listing folders, reading metadata), which C cannot change, and Rust and C compile through the same LLVM backend. The profile showed something else: a third of the instructions went to re-checking every visited path in full against the credential, version-control and system rules (`policy::sensitive_name`, `Scope::inside`), although a walk only enters folders that already passed them.

Two changes, both returning exactly the same decisions:

- **Only what an entry adds is checked.** `Scope::below(root)` keeps only the exclusions and system locations inside the walked root (usually none), and `policy::sensitive_component` checks the entry's own name with its parent. One byte settles most names: every sensitive name starts with `.`, `_`, `{`, `$` or one of a few letters, or ends in a repository-file extension. `checking_only_new_components_matches_the_full_check` compares it with the full check on every reachable path of up to three components built from 32 tricky names, under a user folder, the home folder and `/`.
- **Rule matching settles most folders without allocating.** `match_rules` compares the folder's name with each rule's last name before splitting any path or pattern.

Linux VM, warm cache, `cym scan <tool>` on the benchmark fixture, median of seven runs. Every tool's findings (IDs, sizes and blocked reasons) were identical before and after.

| Tool | Before | After | Wall time | CPU time in the app |
| --- | ---: | ---: | --- | --- |
| Large Files | 832 ms | 616 ms | −26% | −59% |
| Git Worktrees | 323 ms | 232 ms | −28% | −36% |
| Build Artifacts | 305 ms | 226 ms | −26% | −38% |
| Dependencies | 443 ms | 366 ms | −17% | −25% |
| Exact Duplicates | 377 ms | 327 ms | −13% | −19% |
| Storage Explorer | 188 ms | 182 ms | −3% | −3% |

Build Artifacts' instruction count halved (995 M to 494 M before the second change). Storage Explorer lists one level and gains little.

Still open: Build Artifacts searches each matched folder for shipped builds (`shipped_outputs`, up to 20,000 entries) before the sizer walks it again, about 86 ms of its 271 ms here. Folding that search into the size walk, or limiting it to rules whose output can hold a shipped build, would change which folders need confirmation, so it is a decision rather than a refactor.
