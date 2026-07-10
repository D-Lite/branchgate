import { BranchGateLogo } from "./BranchGateLogo";

interface BrandLockupProps {
  size?: number;
  showVersion?: boolean;
  version?: string;
}

/** Matches landing page header: orange mark + lowercase wordmark */
export function BrandLockup({
  size = 18,
  showVersion = false,
  version = "beta",
}: BrandLockupProps) {
  return (
    <div className="brand-lockup">
      <BranchGateLogo size={size} className="brand-lockup-logo" />
      <div className="brand-lockup-text">
        <span className="brand-lockup-name">branchgate</span>
        {showVersion ? (
          <span className="brand-lockup-version mono">{version}</span>
        ) : null}
      </div>
    </div>
  );
}
