import type { Summary } from "./api";
import { CalendarIcon, CheckCircleIcon, PersonIcon } from "./icons";

export function SummaryView({ summary }: { summary: Summary }) {
  return (
    <div className="summary">
      <div className="summary-text">
        {summary.summary
          .split("\n")
          .filter(Boolean)
          .map((paragraph, i) => (
            <p key={i}>{paragraph}</p>
          ))}
      </div>

      {summary.decisions.length > 0 && (
        <div className="summary-block">
          <h4>
            Decisiones <span className="count">{summary.decisions.length}</span>
          </h4>
          <ul className="decisions">
            {summary.decisions.map((decision, i) => (
              <li key={i} className="row-tick">
                <CheckCircleIcon />
                <span>{decision}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {summary.action_items.length > 0 && (
        <div className="summary-block">
          <h4>
            Pendientes <span className="count">{summary.action_items.length}</span>
          </h4>
          <ul className="todos">
            {summary.action_items.map((item, i) => (
              <li key={i} className="row-tick">
                <span className="todo-box" />
                <div>
                  <p>{item.task}</p>
                  {(item.owner || item.due) && (
                    <div className="chips">
                      {item.owner && (
                        <span className="chip">
                          <PersonIcon />
                          {item.owner}
                        </span>
                      )}
                      {item.due && (
                        <span className="chip">
                          <CalendarIcon />
                          {item.due}
                        </span>
                      )}
                    </div>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

export function summaryMeta(summary: Summary): string | null {
  const parts = [
    summary.decisions.length > 0 && plural(summary.decisions.length, "decisión", "decisiones"),
    summary.action_items.length > 0 && plural(summary.action_items.length, "pendiente", "pendientes"),
  ];
  return parts.filter(Boolean).join(" · ") || null;
}

export function summaryText(summary: Summary): string {
  const lines = [summary.summary];
  if (summary.decisions.length > 0) {
    lines.push("", "Decisiones:", ...summary.decisions.map((d) => `- ${d}`));
  }
  if (summary.action_items.length > 0) {
    lines.push(
      "",
      "Pendientes:",
      ...summary.action_items.map((a) => `- ${a.owner ? `${a.owner}: ` : ""}${a.task}${a.due ? ` (${a.due})` : ""}`),
    );
  }
  return lines.join("\n");
}

export const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
