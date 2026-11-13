// The welcome: three short steps. It names the capture hotkey, offers
// Print Screen and launch at sign-in (each applied the moment it is
// switched), and ends with a way into Settings. On a first run it must be
// seen to its last step (`mandatory`); after an upgrade it can be skipped.

import { useEffect, useState } from "react";
import { Keys, Switch } from "../settings/controls";
import { Icon } from "../settings/Icon";
import type { Backend, WelcomeState } from "../settings/backend";

const STEPS = 3;

export function Welcome({ initial, backend }: { initial: WelcomeState; backend: Backend }) {
  const [state, setState] = useState(initial);
  const [step, setStep] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const last = step === STEPS - 1;
  const mayLeave = !state.mandatory || last;

  const close = () => {
    if (mayLeave) void backend.close();
  };
  const next = () => (last ? close() : setStep(step + 1));

  useEffect(() => {
    if (last) void backend.welcomeReachedEnd();
  }, [last, backend]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Enter" && !(event.target instanceof HTMLButtonElement)) {
        event.preventDefault();
        next();
      } else if (event.key === "Escape") {
        close();
      } else if (event.key === "ArrowRight" && !last) {
        setStep(step + 1);
      } else if (event.key === "ArrowLeft" && step > 0) {
        setStep(step - 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Alt+F4 and the taskbar's Close wait for the last step too
  useEffect(() => {
    let release: (() => void) | undefined;
    void backend.onCloseRequested(() => mayLeave).then((unlisten) => {
      release = unlisten;
    });
    return () => release?.();
  }, [backend, mayLeave]);

  const change = async (job: () => Promise<void>) => {
    setError(null);
    setBusy(true);
    try {
      await job();
    } catch (problem) {
      setError(String(problem));
    } finally {
      setBusy(false);
    }
  };

  const setPrintScreen = (phramer: boolean) => {
    setState({ ...state, print_screen: phramer });
    void change(async () => {
      const snapshot = await backend.printScreen(phramer);
      setState((current) => ({ ...current, print_screen: !snapshot.snipping_claims_print_screen }));
    });
  };

  const setStartup = (on: boolean) => {
    setState({ ...state, startup_launch: on });
    void change(async () => {
      const snapshot = await backend.set("startupLaunch", on);
      setState((current) => ({ ...current, startup_launch: Boolean(snapshot.values.startupLaunch) }));
    });
  };

  const major = state.version.split(".")[0];
  const hotkey = state.capture_hotkey;

  return (
    <div className="welcome" data-testid="welcome" data-step={step}>
      <header className="welcome-bar" data-tauri-drag-region>
        <span className="welcome-brand" data-tauri-drag-region>
          Phramer {major}
        </span>
        {mayLeave && (
          <div className="window-buttons">
            <button type="button" className="close" aria-label="Close" title="Close (Esc)" onClick={close}>
              <Icon name="x" />
            </button>
          </div>
        )}
      </header>

      <main className="welcome-page" key={step} data-testid={`welcome-step-${step}`}>
        {step === 0 && (
          <>
            <h1 className="welcome-title">Welcome to Phramer</h1>
            <p className="welcome-lead">Capture any part of the screen, mark it up and share it.</p>
            <div className="hero-keys" aria-label="Capture hotkey">
              {hotkey ? <Keys sequence={hotkey} /> : <span className="set-none">No hotkey set</span>}
              {state.print_screen && (
                <>
                  <span className="hero-or">or</span>
                  <Keys sequence="Print" />
                </>
              )}
            </div>
            <p className="welcome-text">
              Press {hotkey ? "the keys above" : "Print Screen"} anywhere in Windows to start a capture, or click
              Phramer&apos;s icon in the notification area.
            </p>
          </>
        )}

        {step === 1 && (
          <>
            <h1 className="welcome-title">Make it yours</h1>
            <p className="welcome-lead">Each choice applies the moment you switch it.</p>
            <div className="welcome-options">
              <div className="welcome-option" data-testid="welcome-print-screen">
                <div className="set-text">
                  <span className="set-label">Use Print Screen to capture</span>
                  <span className="set-help">
                    Windows opens its Snipping Tool on Print Screen unless this is on. Only your own Windows setting
                    changes; switch it back in Settings at any time.
                  </span>
                </div>
                <Switch checked={state.print_screen} label="Use Print Screen to capture" disabled={busy} onChange={setPrintScreen} />
              </div>
              <div className="welcome-option" data-testid="welcome-startup">
                <div className="set-text">
                  <span className="set-label">Launch at sign-in</span>
                  <span className="set-help">Phramer starts in the background, so the hotkey always works.</span>
                </div>
                <Switch checked={state.startup_launch} label="Launch at sign-in" disabled={busy} onChange={setStartup} />
              </div>
            </div>
            {error && (
              <p className="set-message error" role="alert">
                {error}
              </p>
            )}
          </>
        )}

        {step === 2 && (
          <>
            <h1 className="welcome-title">You&apos;re all set</h1>
            <p className="welcome-lead">A few keys to know on the capture screen:</p>
            <ul className="welcome-tips">
              <li>
                <Keys sequence="E" /> opens the selection in the editor
              </li>
              <li>
                <Keys sequence="Space" /> shows the tool settings
              </li>
              <li>
                <Keys sequence="Ctrl+C" /> copies, <Keys sequence="Ctrl+S" /> saves
              </li>
            </ul>
          </>
        )}
      </main>

      <footer className="welcome-foot">
        <div className="dots" aria-label={`Step ${step + 1} of ${STEPS}`}>
          {Array.from({ length: STEPS }, (_, index) => (
            <span key={index} className={`dot${index === step ? " current" : ""}`} />
          ))}
        </div>
        <div className="welcome-actions">
          {step > 0 && !last && (
            <button type="button" className="text-button" onClick={() => setStep(step - 1)}>
              Back
            </button>
          )}
          {!state.mandatory && !last && (
            <button type="button" className="text-button" onClick={close}>
              Skip
            </button>
          )}
          {last && (
            <button type="button" className="text-button" data-testid="welcome-settings" onClick={() => void backend.openSettings().then(close)}>
              Open Settings
            </button>
          )}
          <button type="button" className="text-button primary" data-testid="welcome-next" onClick={next}>
            {last ? "Start capturing" : "Next"}
            <kbd className="on-primary">Enter</kbd>
          </button>
        </div>
      </footer>
    </div>
  );
}
