Feature: Multi-session honesty
  With tagged sessions: roller selection shows correct session state.
  Without tags: UI explicitly shows unknown mapping.

  Scenario: Single session shows its state directly
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    Then the agent state is "idle"

  Scenario: Multiple sessions without tags degrade to unknown
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    And hook "UserPromptSubmit" arrives for session "s2"
    Then there are 2 sessions
    And the agent state is "unknown"

  Scenario: Ending one session recovers single-session truth
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    And hook "UserPromptSubmit" arrives for session "s2"
    Then the agent state is "unknown"
    When hook "SessionEnd" arrives for session "s1"
    Then there is 1 session
    And the agent state is "running"

  Scenario: Session tag learns correlation
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1" with tag "tag-abc"
    Then session tag "tag-abc" maps to session "s1"

  Scenario: Tagged multi-session resolves via terminal correlation
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1" with tag "tag-a"
    And hook "UserPromptSubmit" arrives for session "s2" with tag "tag-b"
    And terminal 0 has tag "tag-a"
    And terminal 1 has tag "tag-b"
    And terminal 0 is selected
    Then the agent state is "idle"
