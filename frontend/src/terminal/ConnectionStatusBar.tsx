import { connectionStatusView, type ConnectionState } from "./connectionState";

interface Props {
  state: ConnectionState;
}

export default function ConnectionStatusBar({ state }: Props) {
  const view = connectionStatusView(state);

  return (
    <div
      className="status"
      role="status"
      aria-live="polite"
      data-phase={state.phase}
      data-reason={state.reason}
      data-notice={state.notice}
    >
      <span className="status__indicator" aria-hidden="true" />
      <span className="status__primary">{view.primary}</span>
      {view.detail ? <span className="status__separator">{" · "}</span> : null}
      {view.detail ? <span className="status__detail">{view.detail}</span> : null}
    </div>
  );
}
