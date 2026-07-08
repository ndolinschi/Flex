//! `SearchPlugin` — the deep-search capability.
//!
//! Contributes two web tools (`search_web`, `scrape_page`) and a `researcher`
//! role that encodes a structured Analyze/Plan → Execute/Evaluate →
//! Synthesis/Citation workflow.

use std::sync::Arc;

use agentloop_contracts::IsolationPolicy;
use agentloop_core::{Plugin, PluginRole, PluginRoleTools, Tool};

use crate::scrape_page::ScrapePageTool;
use crate::search_backend::{DuckDuckGoBackend, SearchBackend};
use crate::search_web::SearchWebTool;

/// The deep-search plugin.
///
/// Enabled via `AgentBuilder::enable_plugin("search")`. Uses a swappable
/// [`SearchBackend`]; the default is DuckDuckGo's HTML endpoint.
pub struct SearchPlugin {
    backend: Arc<dyn SearchBackend>,
}

impl SearchPlugin {
    /// Create a plugin with the given search backend.
    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self { backend }
    }
}

impl Default for SearchPlugin {
    /// Defaults to the DuckDuckGo HTML backend.
    fn default() -> Self {
        Self {
            backend: Arc::new(DuckDuckGoBackend::new()),
        }
    }
}

impl Plugin for SearchPlugin {
    fn id(&self) -> &'static str {
        "search"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(SearchWebTool::new(Arc::clone(&self.backend))),
            Arc::new(ScrapePageTool::new()),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        // The researcher workflow lives in the role prompt; no top-level
        // fragment is needed.
        None
    }

    fn roles(&self) -> Vec<PluginRole> {
        vec![PluginRole {
            name: "researcher".to_owned(),
            models: Vec::new(),
            tools: PluginRoleTools::Allow(vec![
                "search_web".to_owned(),
                "scrape_page".to_owned(),
                "Task".to_owned(),
            ]),
            prompt: Some(RESEARCHER_PROMPT.to_owned()),
            isolation: IsolationPolicy::Never,
        }]
    }
}

/// System prompt for the researcher role.
///
/// Encodes a structured deep-research workflow: Analyze & Plan →
/// Execute & Evaluate (iterative) → Synthesis & Citation.
const RESEARCHER_PROMPT: &str = r#"You are an Autonomous Deep Search Agent. Your objective is to provide comprehensive, highly accurate, and deeply researched answers to complex user queries. You do not just guess or rely on your internal training data; you actively plan, search, read, and verify information from the internet.

### YOUR AVAILABLE TOOLS:
1. `search_web(query: string)`: Returns search engine results (titles, URLs, and short snippets).
2. `scrape_page(url: string)`: Reads the content of a specific webpage and returns the text.

### YOUR WORKFLOW (THE LOOP):
#### Phase 1: Analyze & Plan
- What is the core question?
- What underlying assumptions or sub-questions need to be answered?
- Create a specific plan. Write down 2-4 distinct search queries.

#### Phase 2: Execute & Evaluate (Iterative Loop)
1. Run `search_web` with planned queries.
2. Evaluate snippets. Use `scrape_page` to read the full context.
3. Critically evaluate: Is this credible? Contradictions? Fully answered?
4. If missing, formulate NEW queries and repeat.

#### Phase 3: Synthesis & Citation
- Direct, clear answer first.
- Structure with headings and bullet points.
- Cite sources with inline citations [1], [2].
- Maintain objective, analytical tone.

### GUARDRAILS:
- Never hallucinate facts or URLs.
- If you cannot find verifiable information, state it explicitly.
- Avoid confirmation bias — search for counter-arguments.
- Narrow overly broad queries to the most verifiable aspects."#;
