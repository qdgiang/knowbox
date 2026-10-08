# Recognizing AI-generated text: a verified catalog of writing-style tells

**Date:** 2026-09-03
**Question:** How can a reader recognize AI-generated text from its writing style — tone, overused words, phrasing patterns, structural tics? Compiled as a checklist to make AI-drafted blog posts (technical/data-engineering blog, Apache Iceberg) sound human.
**Method:** deep-research workflow — 5 search angles (tell inventories, lexical phrasebooks, academic stylometry, forums/reader experience, editing/humanization guides), 20 sources fetched, 93 claims extracted, 25 adversarially verified (3 independent skeptical voters each), 18 confirmed, 7 killed, 7 merged findings. 102 agents, ~1.4M tokens.

## Summary

AI-generated text betrays itself at four levels, and the verified evidence supports a concrete, checkable catalog at each:

1. **Lexical:** a high-density "AI vocabulary" (delve, tapestry, crucial, pivotal, robust, testament, landscape, meticulous) plus formulaic phrases ("Dive into", "Navigating the landscape", "It's important to note", "a stark reminder", "left an indelible mark"), several with empirically measured usage spikes in academic corpora since ChatGPT.
2. **Syntactic:** negative-parallelism constructions ("It's not about X, it's about Y"; "Not just X, but also Y"), rule-of-three triads, em-dash overuse, and copula avoidance ("serves as"/"functions as" instead of plain "is").
3. **Rhetorical:** excessive hedging and weasel words, plus "servile positivity" that sands down specific facts into generic, upbeat generalities.
4. **Structural:** uniform paragraph lengths with tidy wrap-up lines, over-signposting ("Moreover/Furthermore/Consequently"), formulaic openers ("In today's fast-paced world"), and statistically low syntactic variety.

Why a checklist is the right tool: unaided human judges perform poorly (best correct AI-labeling rate 56.8% in a PLOS ONE study, n=403) and rely on exactly these impressionistic surface cues — phraseology, word choice, punctuation — rather than systematic criteria. Readers key off surface tells; editing against an explicit catalog of those tells is the effective approach.

## Findings (verified)

### F1. Unaided human detection is poor and impressionistic → checklist beats gut feel [high]

Zaitsu et al., PLOS ONE (n=403 "AI or Human" task): "overall human AI-detection ability was limited"; best correct AI-labeling rate was 56.8% (Claude 3.5 text), and only 31.5% of participants correctly identified the human-written text. Study 2 found people judge "impressionistically based on linguistically lower-level or surface information" — phraseology, expression, word usage, conjunctions, punctuation — rather than systematic criteria. Caveat: Japanese-language, single-genre study.
Source: https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0335369 (vote 3-0 + 3-0)

### F2. Lexical tells: high-density AI vocabulary + formulaic phrases [high]

High-density "AI vocabulary" plus formulaic phrases: **delve, tapestry, crucial, pivotal, robust, underscore, testament, landscape, intricate/intricacies, meticulous, fostering, vibrant, Additionally (sentence-initial), deep dive, align with, "Dive into", "Navigating the (landscape/complexities of)", "Delving into the intricacies of", "Based on the information provided", "provide a valuable insight", "a stark reminder", "left an indelible mark", "Certainly, here are".**

Wikipedia's "Signs of AI writing" guide has a dedicated "High density of AI vocabulary words" section, explicitly framing **density (not single occurrences)** as "one of the strongest tells for AI use." Empirical backing: Geng & Trotta (arXiv:2409.01754), Kobak et al. (arXiv:2406.07016), and "Why Does ChatGPT Delve So Much?" (arXiv:2412.11385, COLING 2025) document abrupt post-ChatGPT spikes of "delve", "intricacies", "meticulous" in arXiv abstracts. These are probabilistic flags that work in combination — single occurrences are common in human prose.
Sources: https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing ; https://seanjkernan.substack.com/p/13-signs-you-used-chatgpt-to-write ; https://alyssawiens.com/2025-03-27/how-can-you-tell-if-writing-is-ai-generated/ (vote 3-0 + 2-1 + 3-0)

### F3. Syntactic tell: negative parallelisms / contrastive constructions [high]

**"Not just X, but also Y", "Not X, but Y", "X rather than Y", "It's not about X, it's about Y", "no ..., no ..., just ..."** — this covers the "It's not A, it's not B — it's C" pattern. Wikipedia's guide has a standalone "Negative parallelisms" section, labeled "stereotypically an AI sign" (humans use the device too). Independently echoed by The Conversation ("Slanguage: AI's stylistic negation"), multiple blogs, and Reddit threads where users report models defaulting to it despite instructions. The tell is frequency/repetition across a piece, not any single instance.
Sources: https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing ; https://seanjkernan.substack.com/p/13-signs-you-used-chatgpt-to-write (vote 3-0 + 3-0)

### F4. Syntactic/stylistic tells: rule-of-three, em-dash overuse, copula avoidance [high]

- **Rule-of-three lists and triadic phrasing** — GPTZero published on the habit; also flagged by editor inventories.
- **Em-dash overuse** — NYT Magazine (Dec 2025) and The Atlantic (May 2026) covered the fixation; r/Journalism writers report dropping the em-dash habit to avoid suspicion. Over-use, not merely use: one or two em dashes are fine; five per section is the tell.
- **Copula avoidance** — "serves as / stands as / marks / functions as / operates as / represents [a]" instead of plain "is"; the Wikipedia guide cites a >10% drop in those basic verbs in 2023 academic writing.

All three are standalone sections in Wikipedia's "Signs of AI writing" guide.
Sources: https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing ; https://www.onlinewritingclub.com/p/18-signs-you-used-ai-to-write-that ; https://seanjkernan.substack.com/p/13-signs-you-used-chatgpt-to-write (vote 3-0 ×3)

### F5. Rhetorical tells: excessive hedging + servile positivity [high]

- **Excessive hedging/weasel words:** "Typically", "More often than not", "Might be", "Don't always", vague attributions ("experts believe", "industry observers note"). Kernan's Substack calls them "an unwillingness to commit to anything... a sure sign of ChatGPT writing."
- **Servile positivity / smoothing:** sanding down specific, unusual, or nuanced facts into generic, positive descriptions — "the LLM tends to omit specific, unusual, nuanced facts... statistical regression to the mean" (Wikipedia essay via LitHub). Academic corroboration: Sharma et al. (Anthropic, ICLR 2024) on RLHF-driven sycophancy; work on generalization bias.

For a technical blog this is the most actionable pair: commit to concrete numbers and named tools instead of hedged generalities.
Sources: https://seanjkernan.substack.com/p/13-signs-you-used-chatgpt-to-write ; https://www.onlinewritingclub.com/p/18-signs-you-used-ai-to-write-that ; https://lithub.com/heres-a-handy-guide-to-help-you-spot-ai-writing/ (vote 3-0 ×3)

### F6. Structural tells: over-regularity, signposting, filler frames [medium]

Paragraphs of nearly identical length each ending with a tidy concluding line; over-signposting with formal connectives ("Moreover,", "Furthermore,", "Consequently") opening consecutive sentences; filler frames ("it is important to note that", "in today's fast-paced world" — Pangram reports the latter appears ~35x more often in AI text); low syntactic variety / grammatical standardization; formulaic framing ("Whether X or Y, ..."). Low "burstiness" (uniform sentence/paragraph length) is a canonical AI signal per GPTZero and IEEE research. The academic leg (arXiv:2507.00838v2: GPT-4 POS n-gram frequencies "strongly frequency-standardised") is scope-limited to Wikipedia introductions with one genre counter-finding — hence medium confidence.
Sources: https://www.wanderingeducators.com/best/stories/top-signs-writing-generated-by-ai-how-to-fix-it ; https://arxiv.org/html/2507.00838v2 ; https://alyssawiens.com/2025-03-27/how-can-you-tell-if-writing-is-ai-generated/ (vote 3-0 + 3-0 + 2-1)

### F7. Beyond prose style: formatting and residue tells [high]

The Wikipedia community guide catalogs whole non-prose categories: **bullet-point/inline-header lists, emoji as formatting, leftover pasted chatbot correspondence with cheery replies ("I hope this helps", "Of course!", "Certainly!"), Markdown artifacts (bleed-through, ChatGPT contentReference/oaicite bugs), and broken/invalid citations.** Organized into Language/vocabulary, Style, "Communication intended for the user", Markup, Citations. Corroborated by LitHub, Fast Company, MakeUseOf, Ars Technica, Wired. For a blog pipeline the cheapest wins are here: strip chat residue, markdown cruft, and formulaic list formatting before editing prose at all.
Sources: https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing ; https://lithub.com/heres-a-handy-guide-to-help-you-spot-ai-writing/ (vote 3-0 ×2)

## Refuted claims — do NOT cite

These did not survive adversarial verification (≥2/3 skeptical votes refuted):

- "Hundreds of times more likely" word-frequency multipliers (humanizethisai.com — marketing blog of an AI-humanizer vendor; detectors don't work off word lists; GPTZero's detection is probability-based with the word list only a highlight overlay). Vote 0-3 ×3 on three claims from this source.
- The causal claim that AI overuses formal vocabulary "because training data skews formal, hedged, and cautious" — directly contradicted by Juzek & Ward (COLING 2025, arXiv:2412.11385): "We fail to find evidence that lexical overrepresentation is [due to training data]." The word list survives; the mechanism does not. Vote 0-3.
- "AI sentences average ~27 words / uniform long sentence structure" — unsourced blog anecdote universalized; contradicted by Desaire et al. 2023 (PMC10328544): average sentence length was NOT a useful discriminator; only variance/burstiness distinguishes. Vote 0-3.
- "Metronome rhythm: medium-length sentences (12–18 words) with buzzwords at regular intervals" — blog-only, unverifiable. Vote 1-2.
- GPT-4 overuse of "significant/notable/despite" as a standalone lexical tell — overreach; the paper (arXiv:2507.00838v2) is multivariate-stylometry scoped (GPT-4 only, Wikipedia intros); words are SHAP features, not standalone detectors. Vote 1-2.
- A qualitative-review claim (Zenodo record) that AI writing "consistently shows predictable phrasing, structured transitions, consistent hedging" usable as tells — unsupported generalization. Vote 0-3.

## Caveats

- **All tells are probabilistic, never proof.** Nearly every tell (em dashes, tricolons, hedging, contrastive constructions) is also a legitimate human device; the tell is frequency and density, not occurrence.
- **Time-sensitivity is real.** Tells migrate across model generations — even the Wikipedia guide notes "delve" faded by 2025, and multiple 2026 sources argue some classic tells are "dead" on frontier models (GPT-5/Claude 4.x-era). Treat the lexical list as the most volatile layer; structural/rhetorical layers (uniformity, hedging, smoothing) are more durable.
- **Source-quality caveats.** Several confirmed claims rest on blog-level sources (Kernan's Substack is paywalled past the intro; OnlineWritingClub membership-gated) that survived mainly via independent corroboration. The PLOS ONE study used Japanese texts and one genre; the stylometry result is limited to Wikipedia introductions.
- **Use-case warning: over-applying the checklist has its own tells.** Curt fragment sentences used for punch, forced "human-sounding" irregularity, and stripping all structure can read as artificially humanized. Vary sentence length naturally rather than inserting fake fragments.

## Open questions

1. Which tells remain diagnostic for current-generation models (GPT-5 / Claude 4.x era, 2026) versus already-extinct — is there a maintained, model-version-tagged inventory?
2. How do these tells manifest in technical writing (data-engineering blogs, docs)? Do em-dash overuse or rule-of-three pattern differently in code-heavy content explaining things like Apache Iceberg?
3. Does checklist-driven "humanization" editing measurably reduce detection by humans and automated detectors, or does it create a new recognizable style (the "AI-humanized" uncanny valley)?
4. What is the right checklist unit — individual words, or density thresholds (e.g., AI-vocab words per 500 words)? The Wikipedia guide implies density, but no verified source quantifies a threshold.

## Practical checklist: de-AI-ing a blog draft

Ordered cheapest-first, from the findings above:

1. **Strip residue** (F7): pasted chat replies ("Certainly!", "I hope this helps"), Markdown bleed-through, emoji formatting, broken links.
2. **Kill the filler frames** (F6): "It is important to note that", "In today's fast-paced world", "In conclusion", "Moreover/Furthermore/Consequently" stacked at sentence starts.
3. **Hunt the AI vocabulary** (F2): grep for delve, tapestry, crucial, pivotal, robust, underscore, testament, landscape, meticulous, vibrant, leverage, navigate, foster, unlock, seamless. Rule of thumb: 3–4 flagged words in one paragraph → rewrite that paragraph. Density is the tell, not single occurrences.
4. **Break negative parallelisms** (F3): count "not X, but Y" / "it's not about X, it's about Y" / "no ..., no ..., just ..." constructions. One is fine; three in a post is a fingerprint.
5. **Defuse triads and em dashes** (F4): split rule-of-three lists into honest counts (often it's really two items, or four); cap em dashes at ~1 per section; replace "serves as / functions as / stands as" with "is".
6. **Commit** (F5): delete hedges ("typically", "often", "may", "can be") unless uncertainty is real and yours; replace vague attributions ("experts note") with named sources, real numbers, actual error messages, the version you tested. Specificity is the strongest human signal.
7. **Vary the rhythm** (F6): mix sentence lengths by writing them, not by inserting fake fragments. Read the draft aloud; uniform paragraph lengths with tidy wrap-up sentences are the structural tell.
8. **Keep the good parts:** structure, headers, and lists are fine for technical blogs — the tell is *uniformity* and *formula*, not organization.

## Sources (primary ones)

- Wikipedia community guide: https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing (5 verified claims — the richest single catalog)
- Zaitsu et al., PLOS ONE — human AI-detection ability: https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0335369
- Sean Kernan, "13 signs you used ChatGPT": https://seanjkernan.substack.com/p/13-signs-you-used-chatgpt-to-write
- Alyssa Wiens editor inventory: https://alyssawiens.com/2025-03-27/how-can-you-tell-if-writing-is-ai-generated/
- Online Writing Club, "18 signs": https://www.onlinewritingclub.com/p/18-signs-you-used-ai-to-write-that
- Wandering Educators: https://www.wanderingeducators.com/best/stories/top-signs-writing-generated-by-ai-how-to-fix-it
- LitHub guide: https://lithub.com/heres-a-handy-guide-to-help-you-spot-ai-writing/
- Stylometry: arXiv:2507.00838v2 (multivariate POS standardization), arXiv:2412.11385 ("Why Does ChatGPT Delve So Much?", COLING 2025), arXiv:2409.01754 (Geng & Trotta), arXiv:2406.07016 (Kobak et al.), https://academic.oup.com/dsh/advance-article/doi/10.1093/llc/fqag064/8714041
- Forums: r/WritingWithAI megathread on AIisms, r/ChatGPTPro overused-terms thread, r/OpenAI "AI slop" thread, r/AskReddit dead-giveaway thread, HN thread 46646939
