//! Per-model USD pricing, used to turn a turn's accumulated [`crate::TokenUsage`]
//! into an estimated dollar cost.
//!
//! Pure data + pure lookup, no I/O: matches the crate's wasm-compilable,
//! logic-light contract. Provider model ids frequently carry date suffixes
//! (e.g. `claude-opus-4-5-20251101`) or a `provider/` prefix (a [`crate::ModelRef`]
//! rendered to a plain string), so [`price_for`] matches by known id
//! fragments rather than exact equality.
//!
//! Prices as of 2026-07-11 — update this table when providers change list
//! prices, and extend it as new models ship.

/// USD price per token for one model, split by token kind. All rates are
/// already divided down from the usual "per 1M tokens" list price to a
/// per-token rate so [`ModelPrice::cost`] can multiply directly against a
/// [`crate::TokenUsage`] count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    /// USD per input token.
    pub input_per_token: f64,
    /// USD per output token.
    pub output_per_token: f64,
    /// USD per cache-read token (prompt-cache hit). Falls back to
    /// `input_per_token` when a provider doesn't discount cache reads.
    pub cache_read_per_token: f64,
    /// USD per cache-write token (prompt-cache creation). Falls back to
    /// `input_per_token` when a provider doesn't surcharge cache writes.
    pub cache_write_per_token: f64,
}

impl ModelPrice {
    /// Build a price from the list prices providers publish: USD per 1M
    /// input/output tokens, plus optional per-1M cache rates (defaulting to
    /// the input rate when a provider doesn't break them out).
    const fn per_million(
        input: f64,
        output: f64,
        cache_read: Option<f64>,
        cache_write: Option<f64>,
    ) -> Self {
        let input_per_token = input / 1_000_000.0;
        Self {
            input_per_token,
            output_per_token: output / 1_000_000.0,
            cache_read_per_token: match cache_read {
                Some(v) => v / 1_000_000.0,
                None => input_per_token,
            },
            cache_write_per_token: match cache_write {
                Some(v) => v / 1_000_000.0,
                None => input_per_token,
            },
        }
    }

    /// Estimated USD cost of `usage` at this model's rates.
    pub fn cost(&self, usage: &crate::TokenUsage) -> f64 {
        let mut total =
            usage.input as f64 * self.input_per_token + usage.output as f64 * self.output_per_token;
        if let Some(cache_read) = usage.cache_read {
            total += cache_read as f64 * self.cache_read_per_token;
        }
        if let Some(cache_write) = usage.cache_write {
            total += cache_write as f64 * self.cache_write_per_token;
        }
        total
    }
}

/// One entry in the static price table: a lowercase id fragment matched
/// against a lowercased model id (see [`price_for`]), plus its price.
struct PriceEntry {
    /// Matched via `id.contains(fragment)`. List the more specific fragments
    /// first — [`price_for`] returns the first match.
    fragment: &'static str,
    price: ModelPrice,
}

/// Static price table, current as of 2026-07-11. Ordered most-specific
/// fragment first within each family so, e.g., `claude-3-5-haiku` doesn't
/// shadow a hypothetical `claude-3-5-haiku-pro`.
const PRICES: &[PriceEntry] = &[
    // --- Anthropic (Claude), USD / 1M tokens ---
    PriceEntry {
        fragment: "claude-opus-4-6",
        price: ModelPrice::per_million(15.0, 75.0, Some(1.5), Some(18.75)),
    },
    PriceEntry {
        fragment: "claude-opus-4-5",
        price: ModelPrice::per_million(15.0, 75.0, Some(1.5), Some(18.75)),
    },
    PriceEntry {
        fragment: "claude-sonnet-4-5",
        price: ModelPrice::per_million(3.0, 15.0, Some(0.3), Some(3.75)),
    },
    PriceEntry {
        fragment: "claude-sonnet-4",
        price: ModelPrice::per_million(3.0, 15.0, Some(0.3), Some(3.75)),
    },
    PriceEntry {
        fragment: "claude-haiku-4-5",
        price: ModelPrice::per_million(1.0, 5.0, Some(0.1), Some(1.25)),
    },
    PriceEntry {
        fragment: "claude-3-7-sonnet",
        price: ModelPrice::per_million(3.0, 15.0, Some(0.3), Some(3.75)),
    },
    PriceEntry {
        fragment: "claude-3-5-haiku",
        price: ModelPrice::per_million(0.8, 4.0, Some(0.08), Some(1.0)),
    },
    // --- OpenAI, USD / 1M tokens ---
    PriceEntry {
        fragment: "gpt-4.1-mini",
        price: ModelPrice::per_million(0.4, 1.6, Some(0.1), None),
    },
    PriceEntry {
        fragment: "gpt-4.1-nano",
        price: ModelPrice::per_million(0.1, 0.4, Some(0.025), None),
    },
    PriceEntry {
        fragment: "gpt-4.1",
        price: ModelPrice::per_million(2.0, 8.0, Some(0.5), None),
    },
    PriceEntry {
        fragment: "gpt-4o-mini",
        price: ModelPrice::per_million(0.15, 0.6, Some(0.075), None),
    },
    PriceEntry {
        fragment: "gpt-4o",
        price: ModelPrice::per_million(2.5, 10.0, Some(1.25), None),
    },
    PriceEntry {
        fragment: "o4-mini",
        price: ModelPrice::per_million(1.1, 4.4, Some(0.275), None),
    },
    PriceEntry {
        fragment: "o3-mini",
        price: ModelPrice::per_million(1.1, 4.4, Some(0.55), None),
    },
    PriceEntry {
        fragment: "o3",
        price: ModelPrice::per_million(2.0, 8.0, Some(0.5), None),
    },
    // --- Gemini, USD / 1M tokens ---
    PriceEntry {
        fragment: "gemini-2.5-pro",
        price: ModelPrice::per_million(1.25, 10.0, None, None),
    },
    PriceEntry {
        fragment: "gemini-2.5-flash",
        price: ModelPrice::per_million(0.3, 2.5, None, None),
    },
    PriceEntry {
        fragment: "gemini-2.0-flash",
        price: ModelPrice::per_million(0.1, 0.4, None, None),
    },
    PriceEntry {
        fragment: "gemini-1.5-pro",
        price: ModelPrice::per_million(1.25, 5.0, None, None),
    },
    PriceEntry {
        fragment: "gemini-1.5-flash",
        price: ModelPrice::per_million(0.075, 0.3, None, None),
    },
];

/// Look up the USD price for a model id, or `None` for unknown/local models
/// (e.g. Ollama) whose cost can't be estimated. Matches case-insensitively
/// against known id fragments so exact provider snapshot ids (which often
/// carry a date suffix, e.g. `claude-opus-4-5-20251101`) and provider-qualified
/// refs (`"anthropic/claude-sonnet-4-5"`) both resolve.
pub fn price_for(model: &str) -> Option<ModelPrice> {
    let model = model.to_ascii_lowercase();
    PRICES
        .iter()
        .find(|entry| model.contains(entry.fragment))
        .map(|entry| entry.price)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenUsage;

    const EPSILON: f64 = 1e-9;

    #[test]
    fn known_model_times_known_usage_matches_expected_cost() {
        // claude-sonnet-4-5: $3/1M in, $15/1M out.
        // 1_000_000 input + 500_000 output => $3.00 + $7.50 = $10.50
        let usage = TokenUsage {
            input: 1_000_000,
            output: 500_000,
            cache_read: None,
            cache_write: None,
            reasoning: None,
        };
        let price = price_for("claude-sonnet-4-5-20250929").expect("known model");
        let cost = price.cost(&usage);
        assert!((cost - 10.5).abs() < EPSILON, "expected 10.5, got {cost}");
    }

    #[test]
    fn cache_tokens_are_priced_at_their_own_rate() {
        // claude-opus-4-5: $15/1M in, $75/1M out, $1.5/1M cache-read, $18.75/1M cache-write.
        let usage = TokenUsage {
            input: 0,
            output: 0,
            cache_read: Some(1_000_000),
            cache_write: Some(1_000_000),
            reasoning: None,
        };
        let price = price_for("anthropic/claude-opus-4-5").expect("known model");
        let cost = price.cost(&usage);
        assert!((cost - 20.25).abs() < EPSILON, "expected 20.25, got {cost}");
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(price_for("some-local-llama-model").is_none());
        assert!(price_for("ollama/mistral").is_none());
    }
}
