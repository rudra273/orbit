# 02 · What human memory teaches, and what it doesn't

> **What this document decides:** which principles from cognitive science and neuroscience Orbit adopts, which it adapts, and which it deliberately refuses to copy. The brain is a source of design ideas, not a specification: it is optimised for a world where storage is scarce, retrieval is noisy and nobody asks for a citation.

Full citations are in [references](../references.md#cognitive-science-and-neuroscience).

## 1. The governing idea: two learning systems

**Complementary Learning Systems** theory (McClelland, McNaughton & O'Reilly, 1995; updated by Kumaran, Hassabis & McClelland, 2016) explains why brains have a hippocampus *and* a neocortex. A system that learns fast from single experiences must keep them separate to avoid overwriting; a system that extracts general structure must learn slowly across many experiences. The two are coupled by replay: the fast store re-presents experiences to the slow one, largely offline.

This is the single most useful result for Orbit, because it prescribes an architecture rather than a feature:

| Brain | Orbit |
|---|---|
| Hippocampus: fast, sparse, episode-specific, lossless-ish | **Evidence ledger + episodes**: append-only, verbatim, written in milliseconds |
| Neocortex: slow, overlapping, general | **Beliefs and views**: derived, consolidated, revised offline |
| Replay during sleep | **Sleep cycles**: background consolidation with prioritised replay |
| Hippocampal index pointing at cortical patterns (Teyler & DiScenna, 1986) | **Claims and entities as an index** into verbatim evidence — the idea HippoRAG already borrows |

## 2. Principles and their translation

**Adopt** = used close to as described. **Adapt** = the principle is kept, the mechanism changes. Status of the Orbit mechanism is marked **Proven** (established technique), **Adapted** (proven elsewhere, new setting) or **Hypothesis** (ours; must be validated).

| Principle | Finding | Orbit mechanism | |
|---|---|---|---|
| **Multiple memory systems** — Tulving (1972); Squire (2004) | Episodic, semantic and procedural memory dissociate | Seven memory *kinds* over one substrate, not seven databases ([05](../architecture/05-memory-model.md)) | Adapt |
| **Encoding specificity** — Tulving & Thomson (1973); Godden & Baddeley (1975) | Recall succeeds when retrieval cues match the encoding context | Every memory stores its context (project, client, task, time); the *current* context is itself a retrieval cue | Adopt · Adapted |
| **Spreading activation, fan effect** — Collins & Loftus (1975); Anderson (1974) | Activation flows along associations and is diluted by the number of associations a cue has | Typed Spreading Activation with degree normalisation ([08](../architecture/08-retrieval.md)) | Adopt · Hypothesis in this form |
| **Rational analysis of memory** — Anderson & Schooler (1991) | Availability tracks the statistical probability a memory will be needed: recency and frequency, as a power law | Base-level activation from real use events, as a ranking prior | Adopt · Adapted |
| **Storage vs retrieval strength** — Bjork & Bjork (1992) | A memory can be permanently stored yet hard to access; the two strengths are separate | Forgetting changes *accessibility*, never *existence* | Adopt · Adapted |
| **Systems consolidation, replay** — Wilson & McNaughton (1994); Frankland & Bontempi (2005) | Memories are reorganised offline; hippocampal ensembles reactivate in sleep | Sleep cycles ([07](../architecture/07-consolidation.md)) | Adapt · Adapted |
| **Prioritised replay** — Mattar & Daw (2018) | Replay order is explained by *gain × need* | Replay priority = expected change to future answers × likelihood of being needed | Adapt · Hypothesis |
| **Reconsolidation** — Nader, Schafe & LeDoux (2000); Gershman et al. (2017) | Retrieval makes a memory labile; prediction error decides whether it is updated or a new one is formed | A served memory opens a short window in which contradicting evidence is routed straight to it — but the update is a new *version*, never an overwrite | Adapt · Hypothesis |
| **Schemas** — Bartlett (1932); Tse et al. (2007) | Prior structure speeds consolidation of consistent information | Views; schema-congruent claims take a cheap path, surprising ones get scrutiny | Adapt · Hypothesis |
| **Salience, novelty, tagging** — McGaugh (2004); Lisman & Grace (2005); Frey & Morris (1997) | Arousal and novelty gate consolidation; a strong event rescues weak memories formed near it | Salience gating with *spillover* to neighbouring turns ([06](../architecture/06-memory-formation.md)) | Adapt · Hypothesis |
| **Spacing and testing effects** — Cepeda et al. (2006); Roediger & Karpicke (2006) | Spaced repetition and retrieval practice strengthen memory more than massed repetition | Restatements count by *independent* occasion; successful use reinforces | Adapt · Adapted |
| **Working memory** — Baddeley & Hitch (1974); Baddeley (2000); Cowan (2001) | A small, active workspace (about four chunks) bound to long-term memory by an episodic buffer | A capacity-bounded, cross-client workspace | Adapt · Adapted |
| **Autobiographical memory** — Conway & Pleydell-Pearce (2000) | Organised hierarchically: lifetime periods → general events → event-specific knowledge; current goals shape retrieval | Period → project/general event → episode → evidence hierarchy; active goals bias recall | Adopt · Adapted |
| **Event segmentation** — Zacks et al. (2007) | Experience is parsed into events at moments of prediction error | Episode boundaries at topic/goal shifts, not at fixed token counts | Adopt · Adapted |
| **Temporal context** — Howard & Kahana (2002) | Items experienced close in time cue each other | Contiguity edges between temporally adjacent episodes, across sources | Adapt · Adapted |
| **Pattern completion / separation** — Marr (1971); Yassa & Stark (2011) | A partial cue reinstates a whole memory; similar inputs are kept distinct to prevent interference | Episode completion from partial hits; a **confusable registry** that keeps near-identical names apart | Adapt · Hypothesis |
| **Prospective memory** — Einstein & McDaniel (1990) | Intentions are retrieved by time-based or event-based cues | Intentions with triggers, surfaced when the context matches | Adopt · Adapted |
| **Gist and verbatim traces** — Brainerd & Reyna (2002) | Both are stored in parallel; they dissociate | Every memory renderable at several resolutions; the retriever chooses per item | Adopt · Hypothesis |
| **Familiarity vs recollection** — Yonelinas (2002) | A fast familiarity signal and a slower recollective process | Fast path (no LLM) and deep path (planner-driven) | Adapt (loosely) |
| **Metamemory** — Nelson & Narens (1990) | People monitor their own memory: feeling-of-knowing predicts recall | Calibrated recall confidence; abstention as a first-class answer | Adapt · Hypothesis |
| **Traits as distributions** — Fleeson (2001) | A trait is a distribution of states across situations | Identity as context-conditional beliefs with a state/trait stability estimate | Adapt · Hypothesis |

## 3. What Orbit refuses to copy

Human memory is reconstructive: recall rebuilds an experience from fragments and fills gaps with what is plausible. That is why people confidently remember things that never happened. Schacter's *seven sins* (1999) are the cleanest catalogue of these failures, and each maps to something Orbit must engineer **against**.

| Sin | In humans | Countermeasure in Orbit |
|---|---|---|
| **Transience** | Memories fade | Evidence is never deleted by decay; only its accessibility changes |
| **Absent-mindedness** | Never encoded because attention was elsewhere | Deterministic capture; raw evidence stored before any model judges it |
| **Blocking** | It's there but won't come | Several independent retrieval channels and indirect cues, so one failed cue is not a failed recall |
| **Misattribution** | Right content, wrong source (Johnson, Hashtroudi & Lindsay, 1993) | Byte-anchored provenance; speaker attribution checked mechanically; **Echo Guard** for Orbit's own output |
| **Suggestibility** | Post-event information rewrites the memory (Loftus & Palmer, 1974) | Integrity labels; an assistant's suggestion becomes the user's position only through an explicit user ratification |
| **Bias** | The present distorts the remembered past | Bitemporal versions; history is never rewritten |
| **Persistence** | Unwanted memories intrude | User-controlled forgetting, sensitivity ceilings, per-client grants |

The Deese–Roediger–McDermott paradigm (Roediger & McDermott, 1995) deserves special mention: shown a list of related words, people reliably "remember" the unpresented word that ties them together. Embedding retrieval has the same failure — semantically central content looks recalled whether or not it was ever said. Orbit uses this directly as a test ([15](../validation/15-evaluation.md), *lure suite*).

Also rejected:

- **Overwriting, distributed storage.** It is what makes biological forgetting and interference unavoidable. Orbit keeps discrete, addressable, versioned records.
- **Parametric personal memory.** Fine-tuning a model on someone's life produces memory that cannot cite its source, cannot be partially deleted, and dies with the model.
- **Capacity limits as a storage constraint.** They are kept only where they do real work: bounding working memory and context budgets.
- **Emotion as distortion.** Salience may raise how carefully something is consolidated; it must never change what is recorded.

## 4. Three conclusions that shape everything

1. **Separate fast and slow.** Write verbatim immediately; understand later, offline, repeatedly. The fast store makes the system lossless; the slow one makes it intelligent.
2. **Borrow the brain's retrieval, not its storage.** Cue-driven, associative, context-sensitive recall is worth imitating. Reconstructive storage is the thing to avoid.
3. **Treat forgetting as ranking.** Decay, interference and suppression all become statements about *what surfaces first*, so the system can be both selective and complete.

A caution from recent work: a neuroscience-inspired seven-layer memory system was evaluated under a controlled harness and scored the same as far simpler baselines, with its flat variant marginally ahead of the full one ([ZenBrain, 2026](../references.md#systems)). Biological plausibility is not evidence of benefit. Every mechanism above that is marked *Hypothesis* has an ablation in the evaluation plan and is removed if it does not pay for itself.
