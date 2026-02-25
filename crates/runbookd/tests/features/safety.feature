Feature: Safety and dispatch
  The device must be safe by default. Keypad presses never send text.
  Enter dispatches only when armed. Esc cancels arming without interrupting Claude.

  Scenario: Keypad press arms prompt without sending text
    Given a fresh daemon with prompts
    When the operator presses keypad slot "prep_pr"
    Then the daemon is armed with "prep_pr"
    And no text was sent to the terminal

  Scenario: Enter dispatches armed prompt
    Given a fresh daemon with prompts
    And the operator has armed "prep_pr"
    When the operator presses Enter
    Then "/runbook:prep-pr" is sent to the terminal with newline
    And the daemon is no longer armed

  Scenario: Enter passthrough when not armed
    Given a fresh daemon with prompts
    When the operator presses Enter
    Then a literal Enter is sent to the terminal
    And no prompt text was sent

  Scenario: Esc cancels armed state silently
    Given a fresh daemon with prompts
    And the operator has armed "prep_pr"
    When the operator presses Esc
    Then the daemon is no longer armed
    And no escape was sent to the terminal

  Scenario: Esc passthrough when not armed
    Given a fresh daemon with prompts
    When the operator presses Esc
    Then a literal Esc is sent to the terminal

  Scenario: Ctrl+C always forwards
    Given a fresh daemon with prompts
    When the operator presses Ctrl+C
    Then a literal Ctrl+C is sent to the terminal

  Scenario: Export sends with newline
    Given a fresh daemon with prompts
    When the operator presses Export
    Then "/export" is sent to the terminal with newline
