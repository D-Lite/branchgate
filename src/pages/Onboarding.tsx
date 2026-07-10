import { useNavigate } from "react-router-dom";
import { BrandLockup } from "../components/brand/BrandLockup";

interface OnboardingProps {
  onSkip: () => void;
}

export function Onboarding({ onSkip }: OnboardingProps) {
  const navigate = useNavigate();

  return (
    <div className="onboarding">
      <div className="onboarding-inner">
        <div className="onboarding-brand brand-lockup-lg">
          <BrandLockup size={26} />
          <p className="mono onboarding-tagline">beta · selective PR promotion</p>
        </div>

        <p className="onboarding-lead">
          Promote merged pull requests between branches — without merging entire
          branches, and without losing track of what&apos;s already moved.
        </p>

        <div className="onboarding-cards">
          <button type="button" className="card card-interactive ob-card" disabled>
            <span className="ob-card-title">Connect GitHub</span>
            <span className="ob-card-desc">
              OAuth device flow — list orgs, repos, and branches via the API.
            </span>
            <span className="badge mono">coming soon</span>
          </button>
          <button
            type="button"
            className="card card-interactive ob-card"
            onClick={() => navigate("/connect")}
          >
            <span className="ob-card-title">Local git repo</span>
            <span className="ob-card-desc">
              Point at an existing checkout — no GitHub account required.
            </span>
          </button>
        </div>

        <button type="button" className="btn btn-subtle skip-btn" onClick={onSkip}>
          Skip for now
        </button>
      </div>
    </div>
  );
}
