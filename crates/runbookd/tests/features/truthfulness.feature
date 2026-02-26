Feature: Truthfulness
  The device must never claim states it cannot verify.
  Without hooks: only Unknown or Sent.
  With hooks: state transitions match hook events only.

  Scenario: No hooks means unknown state
    Given a fresh daemon with prompts
    Then the agent state is "unknown"

  Scenario: Hooks arriving activates hooks mode
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    Then hooks mode is "active"
    And the agent state is "idle"

  Scenario: Hook lifecycle drives state transitions
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    Then the agent state is "idle"
    When hook "UserPromptSubmit" arrives for session "s1"
    Then the agent state is "running"
    When hook "Notification" arrives with matcher "permission_prompt" for session "s1"
    Then the agent state is "waiting_permission"
    When hook "TaskCompleted" arrives for session "s1"
    Then the agent state is "complete"
    When hook "Stop" arrives for session "s1"
    Then the agent state is "settled"

  Scenario: SessionEnd removes session and latches state
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    Then the agent state is "idle"
    When hook "SessionEnd" arrives for session "s1"
    Then no sessions remain
    And the last ended state is "idle"
