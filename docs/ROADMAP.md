# RustScout Project Roadmap

## 1. **Semantic Code Search (AST Parsing / Language-Aware Searching)**

### Overview
A **semantic code search** feature parses source files into an **Abstract Syntax Tree (AST)**, enabling queries on code structure rather than just text. This allows:
- Precise function/class/variable matching.
- Distinguishing code vs. comments or strings.
- Fewer false positives in doc comments or string literals.

### Implementation Approach
1. **Language-Specific AST Libraries**  
   - For Rust code, use `syn` or `rustc_ast`.  
   - For multi-language or polyglot repos, integrate libraries like **Tree-sitter** for TypeScript, Python, Go, etc.

2. **Design**  
   - Introduce a new module, e.g. `rustscout::search::SemanticSearch`.  
   - Each file’s extension triggers a specialized parser.  
   - A new “AST matcher” processes queries: e.g., "Find function calls named `foo` with 2 parameters."

3. **Code Example** *(pseudocode)*
   ```rust
   pub fn search_ast(config: &SearchConfig) -> Result<(), SearchError> {
       for file in discover_files(&config.root_path) {
           let extension = file.extension();
           match extension {
               Some("rs") => {
                   let ast = parse_rust_file(file)?; // uses `syn` or rustc_ast
                   let matches = ast_match(config.pattern_definitions, &ast);
                   if !matches.is_empty() {
                       // record or return matches
                   }
               },
               _ => {
                   // fallback to standard text-based search
               }
           }
       }
       Ok(())
   }
   ```

4. **Backwards Compatibility**  
   - AST-based search is **optional**—non-AST search remains the default.  
   - If a language parser is unavailable, fallback to text-based searching.

5. **Performance Considerations**  
   - AST parsing is more CPU-intensive.  
   - Cache parsed ASTs to avoid re-parsing unchanged files.  
   - Potentially skip large vendor or build directories.

---

## 2. **IDE Plugins (VS Code, IntelliJ, etc.)**

### Overview
IDE plugins bring RustScout’s power directly into editors—**VS Code**, **IntelliJ**, and so forth—enabling advanced code searches **without leaving** the development environment.

### Implementation Approach
1. **VS Code Extension**  
   - Write a `rustscout-vscode` extension in TypeScript.  
   - Provide a command palette entry (“RustScout: Search…”) that spawns the RustScout CLI or library.

2. **IntelliJ Plugin**  
   - Use JetBrains’ plugin API to integrate.  
   - Launch RustScout as a background process; results flow into IntelliJ’s “Find in Files” style interface.

3. **Code Example** *(VS Code snippet, pseudocode)*
   ```ts
   import * as vscode from 'vscode';
   export function activate(context: vscode.ExtensionContext) {
       let disposable = vscode.commands.registerCommand('rustscout.search', async () => {
           const pattern = await vscode.window.showInputBox({ prompt: 'Enter search pattern' });
           if (pattern) {
               const results = await runRustScout(pattern);
               showResultsInPanel(results);
           }
       });
       context.subscriptions.push(disposable);
   }

   async function runRustScout(pattern: string): Promise<string[]> {
       // spawn rustscout CLI with arguments, parse output
       return [...parsedMatches];
   }
   ```

4. **Backwards Compatibility**  
   - No changes to RustScout core, plugin uses existing CLI or library APIs.

5. **Performance Considerations**  
   - Avoid stalling the IDE UI thread: run searches asynchronously.  
   - Potentially index or cache results for repeated queries.

---

## 3. **Streaming Results for Large Searches**

### Overview
Instead of making users wait until a search completes, **stream** partial results in real time—perfect for **large codebases** or **long-running** queries.

### Implementation Approach
1. **Iterator or Channel-Based Search**  
   - Modify `rustscout::search::engine::search` to yield matches as soon as found.  
   - Expose them via a **Crossbeam** or **std::sync::mpsc** channel.

2. **CLI Streaming Mode**  
   - Provide `--stream` or `--live` flags to flush partial matches line-by-line or JSON chunk-by-chunk.

3. **Code Example** *(pseudocode)*
   ```rust
   pub fn search_streaming(config: &SearchConfig) -> impl Iterator<Item = MatchResult> {
       let (tx, rx) = crossbeam_channel::unbounded();
       for file in discover_files(&config.root_path) {
           let tx_clone = tx.clone();
           std::thread::spawn(move || {
               let results = search_file(file, config);
               for r in results {
                   tx_clone.send(r).ok();
               }
           });
       }
       drop(tx);
       rx.into_iter()  // returns an iterator
   }
   ```

4. **Backwards Compatibility**  
   - Retain the existing batch approach.  
   - New streaming output is **opt-in**.

5. **Performance Considerations**  
   - Slight overhead from concurrency channels.  
   - Gains user feedback early and improves perceived performance.

---

## 4. **Integration with Language Servers (LSP)**

### Overview
Integrate RustScout into **LSP** workflows (e.g., for “Go to Definition” or “Find References”). This complements standard LSP features with code-aware searching across large codebases.

### Implementation Approach
1. **LSP Proxy**  
   - Implement a small server bridging LSP requests (like `workspace/symbol`) to RustScout.  
2. **Conversion**  
   - Convert RustScout’s match results into LSP `SymbolInformation` or `Location`.

3. **Code Example** *(pseudocode for LSP adapter)*
   ```rust
   async fn handle_workspace_symbol(query: &str, ctx: &LspContext) -> LspResult<Vec<SymbolInformation>> {
       let results = rustscout_search(query, &ctx.root_path).await?;
       let symbols = convert_to_lsp_symbols(results);
       Ok(symbols)
   }
   ```

4. **Backwards Compatibility**  
   - LSP integration is optional, no effect on CLI usage.

5. **Performance Considerations**  
   - LSP typically expects near-instant results for references.  
   - Possibly store an index if large repos are frequently queried.

---

## 5. **Pre-commit Hooks for Automated Searches**

### Overview
Teams often want to **fail** commits if certain patterns exist (e.g., “TODO”, “FIXME,” “license placeholders”). This provides **quality gates** at commit time.

### Implementation Approach
1. **Hook Generation**  
   - Add `rustscout generate-hook --pattern "TODO|FIXME"` command to auto-generate a `.git/hooks/pre-commit`.
2. **Check on Commit**  
   - The hook runs `rustscout search -p "TODO" --fail-on-match`. If matches exist, the commit is blocked.

3. **Code Example** *(Generated script)*
   ```bash
   #!/usr/bin/env bash
   echo "Running RustScout pre-commit check..."
   rustscout search -p "TODO" --fail-on-match
   if [ $? -ne 0 ]; then
     echo "Found TODOs! Commit aborted."
     exit 1
   fi
   exit 0
   ```

4. **Backwards Compatibility**  
   - Doesn’t change core search logic—just a new workflow.

5. **Performance Considerations**  
   - Typically fast for local commits.  
   - Optionally skip large directories or rely on incremental search for performance.

---

## 6. **Interactive (TUI) Interface**

### Overview
A **terminal-based** UI (like `htop`) for incremental searching, allowing **scrolling**, **filtering**, and **navigation** in real time.

### Implementation Approach
1. **Use TUI Crates**  
   - **`tui-rs`** or **`crossterm`** for rendering UI in the terminal.  
2. **Streaming + TUI**  
   - Combine with streaming results to show partial matches as they appear.

3. **Code Example** *(pseudocode)*
   ```rust
   fn run_tui() -> Result<()> {
       let mut terminal = tui_rs::Terminal::new(...)?
       loop {
           let user_events = poll_for_user_events();
           let partial_matches = fetch_streamed_matches();
           terminal.draw(|f| {
               // Render partial_matches in a scrollable pane
           });
       }
   }
   ```

4. **Backwards Compatibility**  
   - Provide `rustscout tui` as a new subcommand.

5. **Performance Considerations**  
   - Must handle large result sets: consider pagination or chunking.  
   - Minimal overhead from TUI updates if partial results are huge.

---

## 7. **Web Interface**

### Overview
Host a **local web server** for searching code from the browser—ideal for internal dev teams or those wanting a **collaborative** environment.

### Implementation Approach
1. **Minimal Web Server**  
   - Use frameworks like `warp`, `actix`, or `rocket`.  
   - Expose `/search?pattern=...` endpoints returning JSON or NDJSON streams.
2. **Frontend**  
   - Could be React, Vue, or a simple static HTML + JavaScript for rendering results.

3. **Code Example** *(Rust back-end, pseudocode)*
   ```rust
   async fn search_handler(query: String) -> impl warp::Reply {
       let results = rustscout::search::search_streaming(&SearchConfig::from_query(query));
       warp::reply::json(&results.collect::<Vec<_>>())
   }

   #[tokio::main]
   async fn main() {
       let route = warp::path("search")
           .and(warp::query::raw())
           .and_then(search_handler);
       warp::serve(route).run(([127,0,0,1], 8080)).await;
   }
   ```

4. **Backwards Compatibility**  
   - This is an **optional** mode; no effect if user sticks to CLI.

5. **Performance Considerations**  
   - Possibly high concurrency if many users connect.  
   - Consider authentication or ACL for large code repos.

---

## 8. **Further Memory Optimization & Custom Allocators**

### Overview
For extremely large codebases or specialized environments, using **custom allocators** (like `mimalloc` or `jemalloc`) can cut down fragmentation and overhead.

### Implementation Approach
1. **Profiling**  
   - Determine if default system allocator is a bottleneck.  
2. **Add Feature Flags**  
   - e.g. `--features mimalloc` to switch the global allocator:

   ```rust
   #[cfg(feature = "mimalloc")]
   #[global_allocator]
   static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
   ```

3. **Backwards Compatibility**  
   - Default remains the standard allocator.  
   - No changes for users who don’t enable `mimalloc`.

4. **Performance Considerations**  
   - Gains vary by environment and workload.  
   - Must test thoroughly for regressions.

---

## 9. **Support for GPU-accelerated Regex**

### Overview
Use GPU libraries (NVBIO, custom CUDA kernels, or specialized engines like **Hyperscan**) to accelerate heavy regex matching on large data sets.

### Implementation Approach
1. **GPU-based Search**  
   - Provide `--gpu` or `--accelerated` flags if a GPU is detected.  
2. **Hybrid Approach**  
   - If the pattern is small or the data set is small, CPU might be faster.  
   - For massive data, offload to GPU.

3. **Code Example** *(pseudo)*
   ```rust
   #[cfg(feature="gpu")]
   fn gpu_search(pattern: &str, text: &str) -> Vec<Match> {
       // Send data to GPU, run parallel kernel
       // collect matches
   }
   ```

4. **Backwards Compatibility**  
   - GPU mode is purely **opt-in**.  
   - If no GPU found, fallback to CPU.

5. **Performance Considerations**  
   - Could provide massive speedups for large data sets.  
   - Overhead of copying data to/from GPU.  
   - Not beneficial for small files or minimal patterns.

---

## 10. **Distributed Searching Over Network**

### Overview
Partition large code repositories or numerous files across multiple machines to **distribute** the search load. This yields near-linear scalability if implemented carefully.

### Implementation Approach
1. **Master/Worker Model**  
   - A “master” node receives the query.  
   - Splits file lists among multiple “worker” RustScout instances.  
   - Aggregates partial results.

2. **Implementation**  
   - Provide a gRPC/HTTP server in `rustscout::distributed`.  
   - Possibly reuse the streaming code to send partial results back to master.

3. **Code Example** *(pseudocode)*
   ```rust
   async fn distribute_search(files: Vec<PathBuf>, query: &str) -> Vec<MatchResult> {
       let chunked = chunk_files(files, worker_count());
       let futures = chunked.into_iter().map(|chunk| async move {
           let worker_url = pick_worker();
           send_search_request(worker_url, chunk, query).await
       });
       let partials = futures::future::join_all(futures).await;
       partials.concat()
   }
   ```

4. **Backwards Compatibility**  
   - Could remain a “distributed mode” or an optional subcommand.  
   - Local searches are unchanged.

5. **Performance Considerations**  
   - Potential network overhead.  
   - Gains in parallel I/O and CPU across many machines.

---

## 11. **Search History & Favorites**

### Overview
Save prior searches in a local store (JSON, SQLite, or config file). Users can quickly recall or mark certain queries as “favorites.”

### Implementation Approach
1. **History Storage**  
   - A small JSON or SQLite DB to store patterns, timestamps, and “favorite” flags.  
2. **CLI or TUI**  
   - Provide a `--history` or interactive prompt to list prior queries.

3. **Code Example**
   ```rust
   fn add_to_history(pattern: &str) {
       let mut hist = load_history();
       if !hist.contains(pattern) {
           hist.push(pattern.to_string());
       }
       save_history(hist);
   }
   ```

4. **Backwards Compatibility**  
   - Feature is optional—existing usage unaffected.  
   - Could be toggled on/off in config.

5. **Performance Considerations**  
   - Minimal overhead—just a small file or DB read/write.

---

## 12. **Search Templates**

### Overview
Define common or repeatable searches (e.g., “todo,” “fixme,” “license_check”) in a template file. Then run them quickly by name.

### Implementation Approach
1. **Template Definition**  
   - Store templates in `~/.rustscout/templates.toml`, for instance:

     ```toml
     [templates]
     fixme = { pattern = "FIXME", boundary_mode = "WholeWords" }
     suspicious = { pattern = "HACK|TODO|FIXME", is_regex = true }
     ```
2. **CLI Usage**  
   - `rustscout search --template fixme`.

3. **Code Example**
   ```rust
   fn load_templates() -> HashMap<String, SearchConfig> {
       // parse templates.toml into a map
   }

   fn search_with_template(name: &str) -> Result<()> {
       let templates = load_templates();
       let cfg = templates.get(name).ok_or_else(...)?;
       search(cfg)
   }
   ```

4. **Backwards Compatibility**  
   - If no templates file is found, fallback to manual patterns.  
   - No user impact if not used.

5. **Performance Considerations**  
   - Minor overhead loading templates.  
   - Quick or negligible for typical usage.

---

## 13. **Improved Error Messages / Suggestions**

### Overview
Enrich user feedback. For instance, if a user types `-r test*` but forgets quotes, suggest `--regex "test.*"`.

### Implementation Approach
1. **Custom Error Hooks**  
   - Catch `SearchError::PatternError(e)` or similar.  
   - Provide hints (“Did you mean `\"test*\"`?”).
2. **Spell Checking**  
   - Possibly check for “fuzzy” matched flags or patterns.

3. **Code Example**
   ```rust
   match search(config) {
       Err(SearchError::PatternError(e)) => {
           eprintln!("Error with pattern: {}", e);
           if likely_forget_quotes(&e) {
               eprintln!("Hint: Try quoting your pattern like \"test*\".");
           }
       },
       _ => {}
   }
   ```

4. **Backwards Compatibility**  
   - Non-breaking. Just better user guidance.

5. **Performance Considerations**  
   - Minimal overhead.  
   - String checks for common mistakes.

---

## 14. **Structural (Pattern) Search**

### Overview
Generalize beyond AST-based searching. For example, parse JSON, YAML, or custom domain files, letting users define “match a node with property X = Y.”

### Implementation Approach
1. **Generic Tree Representation**  
   - Possibly rely on **Tree-sitter** or other multi-format parser.  
2. **Mini DSL**  
   - e.g., `structural_search --query "fn_with_args(2) inside loop"` for code.  
   - Or `structural_search --query "yaml_key('deployment')"` for YAML.

3. **Code Example** *(pseudocode)*
   ```rust
   fn structural_search(tree: &Tree, query: &str) -> Vec<Match> {
       let parsed_query = parse_query_dsl(query);
       apply_tree_sitter_query(tree, parsed_query)
   }
   ```

4. **Backwards Compatibility**  
   - Adds a new search mode. Text-based search remains default.

5. **Performance Considerations**  
   - Building parse trees can be resource-intensive.  
   - Gains in accuracy for advanced usage.

---

# Implementation Steps for All Features

1. **Plan & Propose**  
   - Create GitHub Issues or design docs for each feature.  
   - Discuss feasibility, gather community feedback.

2. **Start with High-Impact**  
   - For instance, focus on **Semantic Code Search**, **IDE Plugins**, and **Streaming** results first.  
   - Maintain separate branches, merging frequently to keep code stable.

3. **Ensure Backwards Compatibility**  
   - Use optional flags or subcommands.  
   - Keep the standard CLI usage intact.

4. **Add Tests**  
   - Each new feature should have unit tests (module-level) + integration tests (CLI-level).  
   - Use Criterion or cargo bench for performance-critical features (GPU or distributed).

5. **Document Thoroughly**  
   - Expand `docs/` with a dedicated doc for each major feature (AST searching, GPU usage, distributed, etc.).  
   - Update the Developer Guide to mention any new modules or flags.

6. **Performance Validate**  
   - Run large real-world tests to confirm no regressions.  
   - Compare memory usage, search speed, concurrency overhead, etc.

7. **Release & Communicate**  
   - Tag new versions (e.g., `v1.1.0`, `v1.2.0`) once a feature is production-ready.  
   - Provide migration notes if defaults change or if a new subcommand is introduced.
