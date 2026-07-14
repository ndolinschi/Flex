//! `SearchPlugin` — the deep-search capability.
//!
//! Contributes two web tools (`search_web`, `scrape_page`) and a `researcher`
//! role that encodes a structured deep-research workflow with iterative
//! reflection, parallel fan-out, incremental compaction, and layered search.

use std::sync::Arc;

use agentloop_contracts::{IsolationPolicy, ModelRef};
use agentloop_core::{Plugin, PluginRole, PluginRoleTools, Tool};

use crate::rerank::KeywordReranker;
use crate::scrape_page::ScrapePageTool;
use crate::search_backend::{FallbackSearchBackend, SearchBackend, default_search_backends};
use crate::search_web::SearchWebTool;

/// The deep-search plugin.
///
/// Enabled via `AgentBuilder::enable_plugin("search")`. Uses a swappable
/// [`SearchBackend`]; the default is Instant Answer + Wikipedia (optional
/// Brave / SearXNG via env) with keyword-based result re-ranking.
pub struct SearchPlugin {
    backend: Arc<dyn SearchBackend>,
    /// Ordered model preference for the `researcher` role; empty = inherit.
    researcher_models: Vec<ModelRef>,
}

impl SearchPlugin {
    /// Create a plugin with the given search backend.
    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self {
            backend,
            researcher_models: Vec::new(),
        }
    }

    /// Pin the `researcher` role to an ordered model preference chain
    /// (e.g. a cheap/fast model). Empty keeps the default inherit-session
    /// behavior.
    pub fn with_researcher_models(mut self, models: Vec<ModelRef>) -> Self {
        self.researcher_models = models;
        self
    }
}

impl Default for SearchPlugin {
    /// Defaults to Instant Answer + Wikipedia (and optional Brave/SearXNG).
    /// Public SearXNG instances are *not* hard-coded — they 429 constantly
    /// and made `search_web` look permanently rate-limited.
    fn default() -> Self {
        Self {
            backend: Arc::new(FallbackSearchBackend::new(default_search_backends())),
            researcher_models: Vec::new(),
        }
    }
}

impl Plugin for SearchPlugin {
    fn id(&self) -> &'static str {
        "search"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(
                SearchWebTool::new(Arc::clone(&self.backend))
                    .with_reranker(Arc::new(KeywordReranker::new())),
            ),
            Arc::new(ScrapePageTool::new()),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        None
    }

    fn roles(&self) -> Vec<PluginRole> {
        vec![PluginRole {
            name: "researcher".to_owned(),
            models: self.researcher_models.clone(),
            tools: PluginRoleTools::Allow(vec![
                "search_web".to_owned(),
                "scrape_page".to_owned(),
                "Agent".to_owned(),
            ]),
            prompt: Some(RESEARCHER_PROMPT.to_owned()),
            isolation: IsolationPolicy::Never,
        }]
    }
}

/// System prompt for the researcher role.
///
/// Encodes a structured deep-research workflow: Analyze & Plan →
/// Execute & Evaluate with mandatory reflection checkpoints →
/// Synthesis & Citation. Includes instructions for parallel fan-out,
/// incremental compaction, and layered search patterns.
const RESEARCHER_PROMPT: &str = r#"You are an Autonomous Deep Search Agent. Your objective is to provide comprehensive, highly accurate, and deeply researched answers to complex user queries. You do not just guess or rely on your internal training data; you actively plan, search, read, and verify information from the internet.

### YOUR AVAILABLE TOOLS:
1. `search_web(query, max_results?, depth?)`: Search the web. Set `max_results` (1-20, default 15) to control result count. Set `depth` to `"broad"` for exploratory overview searches or `"specific"` for narrowly targeted queries.
2. `scrape_page(url, max_bytes?)`: Read the full content of a specific webpage.
3. `Agent(role="researcher", prompt="...")`: Spawn a parallel researcher sub-agent to investigate a sub-question independently.

### YOUR WORKFLOW (THE LOOP):

#### Phase 1: Analyze & Plan
- What is the core question?
- What underlying assumptions or sub-questions need to be answered?
- For complex questions, identify distinct angles that can be researched in parallel.
- Create a specific plan. Write down 2-4 distinct search queries.

#### Phase 2: Execute & Evaluate (Iterative Loop)
Use a **layered search pattern**:
1. **Broad searches** (2-3 rounds): Start with `depth="broad"` queries to map the landscape. Read snippets; scrape only the most promising pages.
2. **Deep-dive scrapes**: After identifying key sources, scrape them in full. Extract facts, data points, and arguments.
3. **Verification searches** (2-3 rounds): Use `depth="specific"` queries to verify key claims. Search for counter-arguments and contradictory evidence.

**Parallel fan-out for complex questions**: If the question has multiple independent angles (e.g., "analyze the economic and environmental impact of X"), use `Agent(role="researcher", prompt="...")` to spawn parallel researcher sub-agents for each angle. Each sub-agent will return a synthesis of its findings.

**Incremental compaction**: After 3 search rounds, summarize what you have learned concisely before continuing. Discard raw search output that is no longer needed; keep only key facts, sources, and open questions.

#### Reflection Checkpoint (mandatory — after every 2 search+scrape cycles):
Stop and answer these four questions explicitly:
1. What is answered fully?
2. What is partially answered (and what's missing)?
3. Are there contradictions between sources? (List them)
4. What new search queries are needed?

If all questions are answered and no contradictions remain, proceed to Synthesis.

#### Phase 3: Synthesis & Citation
- Direct, clear answer first.
- Structure with headings and bullet points.
- Cite sources with inline citations [1], [2].
- Maintain objective, analytical tone.
- List sources at the end with URLs.

### GUARDRAILS:
- Never hallucinate facts or URLs.
- If you cannot find verifiable information, state it explicitly.
- Avoid confirmation bias — search for counter-arguments and contradictory evidence.
- Narrow overly broad queries to the most verifiable aspects.
- Prefer primary sources (official docs, academic papers, .gov/.edu domains) over secondary commentary."#;
