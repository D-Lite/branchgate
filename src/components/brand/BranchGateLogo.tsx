import type { SVGProps } from "react";

type BranchGateLogoProps = SVGProps<SVGSVGElement> & {
  size?: number | string;
  title?: string;
};

/**
 * BranchGate mark: three source branches converging into one target.
 * Path geometry: /public/brand/branchgate-logo-paths.csv
 */
export function BranchGateLogo({
  size = 18,
  className,
  title = "BranchGate",
  ...props
}: BranchGateLogoProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      role="img"
      aria-label={title}
      {...props}
    >
      <title>{title}</title>
      <path
        d="M5 5C9 5 11 12 14 12"
        stroke="currentColor"
        strokeWidth="1.85"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M5 12H14"
        stroke="currentColor"
        strokeWidth="1.85"
        strokeLinecap="round"
      />
      <path
        d="M5 19C9 19 11 12 14 12"
        stroke="currentColor"
        strokeWidth="1.85"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M14 12H19"
        stroke="currentColor"
        strokeWidth="1.85"
        strokeLinecap="round"
      />
      <circle cx="5" cy="5" r="2.15" fill="currentColor" />
      <circle cx="5" cy="12" r="2.15" fill="currentColor" />
      <circle cx="5" cy="19" r="2.15" fill="currentColor" />
      <circle cx="20.25" cy="12" r="2.65" fill="currentColor" />
    </svg>
  );
}
