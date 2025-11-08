## Tips to write good code documentation

### Document for posterity

- Write docs properly the first time on an assumption that they won't be rewritten later.

- Drive to document the telos (purpose/why) over the technos (the constative facts/what).
  Separate these clearly. Aim to do the former well so the latter will be self-explanatory.

### Document purpose

- I prefer for a docstring to record the purpose of each part, not merely the connectivity.
  This will typically mean describing what something _enables_ (for goal-oriented readers)
  rather than merely describing what it _is_ (the constative facts of the matter).

- Docstrings should tell you what you may not surmise about the purpose of a component by reading
  it, for non-obvious aspects. Reveal implications that aren't immediately apparent on reading.

- Explain the alternative: what would happen without this component, or what problem's solution
  hinges on it?

#### Questions to extract a purpose

- What is the point of this component, what's its _telos_ not merely its _technos_?

- What is something not readily apparent that I can convey to the reader succinctly?

- Are there any contexts in which this component operates differently? To what end?

### Writing style

- Aim for a straightforward, succinct, spirited explanation of your software at a high level.
  Do not over-document, brevity is the soul of wit and code should be considered transient.

- Use active, functional language: what the component "tracks," "manages," "handles," "maintains," "enforces."
  Avoid passive constructions like "is used for" or "contains".

- Never say that something "exists" (this is the epitome of a constative factual style). It is
  notable for conveying no information that cannot be gained from the code, therefore it is a form
  of rework, or wastefulness.

- Avoid meta-commentary, including instructions to "note that...", where you could simply state.

- Avoid hedging language ("may", "might", "could be seen as"), write what is known in certainty.

- Avoid anthropomorphism, especially about errors (avoid calling them "bugs"). A program with an error is
  simply "wrong". See quotations below by E. W. Dijkstra for more detail on this.

#### Dijkstra on style

Excerpts from _On the cruelty of really teaching computing science_
(via https://www.cs.utexas.edu/~EWD/transcriptions/EWD10xx/EWD1036.html)

Dijkstra on errors:

> We could, for instance, begin with cleaning up our language by no longer calling a bug a bug but
> by calling it an error. It is much more honest because it squarely puts the blame where it
> belongs, viz. with the programmer who made the error. The animistic metaphor of the bug that
> maliciously sneaked in while the programmer was not looking is intellectually dishonest as it
> disguises that the error is the programmer's own creation. The nice thing of this simple change of
> vocabulary is that it has such a profound effect: while, before, a program with only one bug used
> to be "almost correct", afterwards a program with an error is just "wrong" (because in error).

Dijkstra on anthropomorphism:

> My next linguistical suggestion is more rigorous. It is to fight the
> "if-this-guy-wants-to-talk-to-that-guy" syndrome: never refer to parts of programs or pieces of
> equipment in an anthropomorphic terminology, nor allow your students to do so. This linguistical
> improvement is much harder to implement than you might think, and your department might consider
> the introduction of fines for violations, say a quarter for undergraduates, two quarters for
> graduate students, and five dollars for faculty members: by the end of the first semester of the
> new regime, you will have collected enough money for two scholarships.
>
> The reason for this last suggestion is that the anthropomorphic metaphor —for whose introduction
> we can blame John von Neumann— is an enormous handicap for every computing community that has
> adopted it. I have now encountered programs wanting things, knowing things, expecting things,
> believing things, etc., and each time that gave rise to avoidable confusions. The analogy that
> underlies this personification is so shallow that it is not only misleading but also paralyzing.
>
> It is misleading in the sense that it suggests that we can adequately cope with the unfamiliar
> discrete in terms of the familiar continuous, i.e. ourselves, quod non. It is paralyzing in the
> sense that, because persons exist and act in time, its adoption effectively prevents a departure
> from operational semantics and thus forces people to think about programs in terms of
> computational behaviours, based on an underlying computational model. This is bad, because
> operational reasoning is a tremendous waste of mental effort.
