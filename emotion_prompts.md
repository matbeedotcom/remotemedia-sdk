System prompts used to generate emotion deflection datasets
For each scenario, we usually have an unexpressed real emotion and a displayed emotion in the content (except the naturally expressed scenario), and a topic. The names in the conversations are randomly sampled.

Prompts for generating naturally expressed emotion transcripts
Generate a scenario AND a dialogue between {NAME_A} and {NAME_B}.

IMPORTANT: You must generate BOTH parts:

1. First, write a scenario description

2. Then, write the dialogue


Format:

Scenario: {NAME_A} feels {REAL_EMOTION} about {TOPIC}. Include context for why they feel this way.


{NAME_A}: [utterance]


{NAME_B}: [response]


...


Requirements:

1. MUST include scenario description before the dialogue

2. Either {NAME_A} or {NAME_B} may speak first in the dialogue

3. Format each turn as "\\n\\n{{Name}}: [text]"

4. Dialogue length is organic - 1-2 turns is enough, but can be more as needed

5. {NAME_A}'s dialogue should naturally reflect {REAL_EMOTION} - the conversation is consistent with this emotion

6. Keep it natural and grounded


Generate with:

- Topic: {TOPIC}

- {NAME_A}'s emotion: {REAL_EMOTION}

Prompts for generating emotion deflection transcripts
Generate a scenario AND a dialogue between {NAME_A} and {NAME_B}.


IMPORTANT: You must generate BOTH parts:

1. First, write a scenario description

2. Then, write the dialogue


Format:

Scenario: Describe where {NAME_A} genuinely feels {REAL_EMOTION} but appears {DISPLAYED_EMOTION} about {TOPIC}. Must explicitly state {NAME_A}'s real emotion. Include context for why they want to conceal.


{NAME_A}: [utterance]


{NAME_B}: [response]


...


Requirements:

1. MUST include scenario description before the dialogue

2. Either {NAME_A} or {NAME_B} may speak first in the dialogue

3. Format each turn as "\\n\\n{{Name}}: [text]"

4. Dialogue length is organic - 1-2 turns is enough, but can be more as needed

5. {NAME_A}'s words should fully reflect {DISPLAYED_EMOTION} with no hints of {REAL_EMOTION}. The hidden emotion exists only in the scenario.

6. Keep it natural and grounded


Generate with:

- Topic: {TOPIC}

- {NAME_A}'s real emotion: {REAL_EMOTION}

- {NAME_A}'s displayed emotion: {DISPLAYED_EMOTION}

Prompts for generating unexpressed emotion (neutral topic) transcripts
In this scenario, the following conversations are some emotion-neutral commonsense dialogues. We only generate the system prompt to reveal the real emotion, then transition to the conversation topic, and then connect with the dialogues."

Generate a brief scenario (2-4 sentences) where {NAME_A} genuinely feels {REAL_EMOTION}, ending with their friend {NAME_B} asking about a different topic.


Scenario context: {TOPIC}

The topic {NAME_B} will ask about: {CONVERSATION_TOPIC}


Requirements:

1. Describe a situation related to "{TOPIC}" that makes {NAME_A} feel {REAL_EMOTION}

2. Explicitly state that {NAME_A} feels {REAL_EMOTION}

3. End with {NAME_B} asking {NAME_A} about the conversation topic (e.g., "Then {NAME_B} asks {NAME_A} about..." or "{NAME_B} turns to {NAME_A} with a question about...")

4. Keep it concise - just the scenario description, no dialogue


Output only the scenario description, nothing else.

Prompts for generating unexpressed emotion (story writing) transcripts
Generate a scenario AND a story written by {NAME_A}.


IMPORTANT: You must generate BOTH parts:

1. First, write a scenario description stating {NAME_A}'s emotional state

2. Then, write the story {NAME_A} tells


Format:

Scenario: {NAME_A} is feeling {REAL_EMOTION} about {TOPIC}. They write/tell a story.]


{NAME_A}: [The story goes here, featuring characters who show {STORY_EMOTION}...


Requirements:

1. MUST include scenario description before the story

2. The scenario must explicitly state {NAME_A}'s {REAL_EMOTION} emotional state

3. After the scenario, {NAME_A} writes/tells the story

4. The story should have characters clearly showing {STORY_EMOTION}

5. The story's emotion ({STORY_EMOTION}) is different from {NAME_A}'s real emotion ({REAL_EMOTION})

6. The story can be any genre: fiction, memoir, creative writing, etc.

7. Keep the story grounded and natural


Generate with:

- Topic/context: {TOPIC}

- {NAME_A}'s real emotion: {REAL_EMOTION}

- Emotion in the story: {STORY_EMOTION}

Prompts for generating unexpressed emotion (discussing others) transcripts
Generate a scenario AND a dialogue between {NAME_A} and {NAME_B}.


IMPORTANT: You must generate BOTH parts:

1. First, write a scenario description

2. Then, write the dialogue


Format:

Scenario: {NAME_A} feels {REAL_EMOTION} about {TOPIC}.


(In the conversation, they discuss someone else who is experiencing {OTHER_EMOTION}.)


{NAME_A}: [utterance]


{NAME_B}: [response]


...


Requirements:

1. MUST include scenario description before the dialogue

2. Either {NAME_A} or {NAME_B} may speak first in the dialogue

3. Format each turn as "\\n\\n{{Name}}: [text]"

4. Dialogue length is organic - 1-2 turns is enough, but can be more as needed

5. CRITICAL: {NAME_A}'s {REAL_EMOTION} exists ONLY in the scenario description. In the dialogue, {NAME_A} hides their emotion completely.

6. CRITICAL: {NAME_A} must explicitly discuss or mention someone else's {OTHER_EMOTION}. The person can be {NAME_B} or any other person.

7. {NAME_A}'s dialogue should be neutral about themselves while focusing on discussing the other person's emotion

8. Keep it natural and grounded

Generate with:

- Topic: {TOPIC}

- {NAME_A}'s real emotion (hidden, only in scenario): {REAL_EMOTION}

- Discussed person's emotion: {OTHER_EMOTION}

