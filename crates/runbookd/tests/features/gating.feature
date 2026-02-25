Feature: Gating is real
  Denied Bash patterns reliably block and the device reflects BLOCKED state.

  Scenario: RunbookPolicy blocked sets agent state
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    And hook "RunbookPolicy" arrives with matcher "blocked" for session "s1"
    Then the agent state is "blocked"

  Scenario: Blocked state clears on next running event
    Given a fresh daemon with prompts
    When hook "Notification" arrives with matcher "idle_prompt" for session "s1"
    And hook "RunbookPolicy" arrives with matcher "blocked" for session "s1"
    Then the agent state is "blocked"
    When hook "UserPromptSubmit" arrives for session "s1"
    Then the agent state is "running"
