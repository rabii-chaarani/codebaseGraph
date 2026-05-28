# Graph Search Output Token Comparison

## Method
- Raw format: compact JSON emitted by the current graph-search payload serializer, counted from the exact serialized JSON text with sorted keys and compact separators.
- Ontology-preserving block format: grouped `file path` blocks with readable `Class`, `Method`, `Scope`, relation, `label`, `span`, `id`, and `rank_score` terms left literal.
- Tokenizer/model: encoding `o200k_base`.
- Count method: payload-only tokens using `len(encoding.encode(text))`; chat-message wrapper tokens were not included.

## Results
| Query | Results | Context edges | Raw tokens | Block tokens | Saved tokens | Reduction % |
|---|---:|---:|---:|---:|---:|---:|
| SearchService | 3 | 6 | 502 | 234 | 268 | 53.4% |

## Aggregate Summary
- Samples: 1
- Total raw tokens: 502
- Total block tokens: 234
- Total saved tokens: 268
- Overall reduction: 53.4%
- Mean reduction: 53.4%
- Median reduction: 53.4%
- Min reduction: SearchService (53.4%)
- Max reduction: SearchService (53.4%)
- p90 raw/block tokens: not reported because fewer than 10 samples were compared

## Ontology Preservation
The validator normalizes raw JSON and block output into canonical result records preserving `type`, `label`, `path`, `span`, `id`, `rank_score`, and ordered context records with `direction`, `relation`, `type`, `label`, `path`, `span`, and non-boilerplate `summary`.

Intentional omissions:
- `results[0].context[0].summary`
- `results[1].context[0].summary`
- `results[2].context[0].summary`

Known limitations: the block parser validates the supported graph-search fixture shape and live graph-search output shape; it is not a general-purpose parser for hand-written variants.

## Recommendation
Use the ontology-preserving block format by default for agent-facing graph-search output when consumers need readable context. Keep JSON available for machine APIs and tests that require strict structured payloads.
