// The console's stylesheet: mockups/console.css is the one source for the static
// mockups, the subset linter and the application.
import css from "../../mockups/console.css" with { type: "text" };

const CONSOLE_SHEET = 1;

export function installStyles(): void {
  View.stylesheet(CONSOLE_SHEET, css);
}
