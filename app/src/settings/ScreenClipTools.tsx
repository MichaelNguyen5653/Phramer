// The MS-SCREENCLIP row, in Settings > Advanced and in the welcome
// (15.0.0's GeneralConf::initScreenClipProtocol and WelcomeTour page).

import { useEffect, useState } from "react";
import type { Backend } from "./backend";
import { Icon } from "./Icon";
import { screenClipRow, toggledMessage, type ScreenClipStatus } from "./screenclip";

/** The MS-SCREENCLIP row: where things stand, Register or Unregister
 * through UAC, and Windows Settings for the choice only the user can make. */
export function ScreenClipTools({ backend }: { backend: Backend }) {
  const [status, setStatus] = useState<ScreenClipStatus | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  useEffect(() => {
    void backend.screenClipStatus().then(setStatus);
  }, [backend]);
  // The Store edition without the declaration has nothing to offer
  if (!status || (status.store && !status.registered)) return null;
  const row = screenClipRow(status);
  const toggle = async () => {
    setWaiting(true);
    setMessage(null);
    try {
      const toggled = await backend.screenClipToggle();
      setStatus(toggled.status);
      setMessage(toggledMessage(toggled));
    } catch (problem) {
      setMessage({ text: String(problem), error: true });
    } finally {
      setWaiting(false);
    }
  };
  return (
    <div className="file-tools" data-testid="screenclip">
      <span role="status">{waiting ? "Waiting for administrator approval…" : row.text}</span>
      <div className="file-buttons">
        {row.settings && (
          <button type="button" className="text-button" onClick={() => void backend.screenClipOpenSettings()}>
            <Icon name="settings" />
            Windows Settings
          </button>
        )}
        {row.button && (
          <button type="button" className="text-button" disabled={waiting} onClick={() => void toggle()}>
            {/* The shield says a UAC prompt follows, as Windows does */}
            <Icon name="shield" />
            {row.button}
          </button>
        )}
      </div>
      {message && (
        <p className={`message${message.error ? " error" : ""}`} role={message.error ? "alert" : "note"}>
          {message.text}
        </p>
      )}
    </div>
  );
}
