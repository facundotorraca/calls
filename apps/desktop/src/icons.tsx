// Small stroke icons, drawn with currentColor so they follow the text color. Corners are 90°.
import type { ReactNode } from "react";

function Icon({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <svg
      className={`icon ${className ?? ""}`}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  );
}

type Props = { className?: string };

export const ChevronIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M6 4l4 4-4 4" />
  </Icon>
);

export const FolderIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M2 4.5c0-.6.4-1 1-1h3l1.5 1.5H13c.6 0 1 .4 1 1V12c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1z" />
  </Icon>
);

export const TrayIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M2 9l1.5-5.5h9L14 9v3.5c0 .3-.2.5-.5.5h-11a.5.5 0 01-.5-.5z" />
    <path d="M2 9h3.5l1 1.5h3l1-1.5H14" />
  </Icon>
);

export const InboxIcon = (p: Props) => (
  <Icon {...p}>
    <rect x="2.5" y="3" width="11" height="10" />
    <path d="M2.5 9.5h3l1 1.5h3l1-1.5h3" />
  </Icon>
);

export const PlusIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M8 3.5v9M3.5 8h9" />
  </Icon>
);

export const SearchIcon = (p: Props) => (
  <Icon {...p}>
    <circle cx="7" cy="7" r="4.25" />
    <path d="M10.2 10.2L13.5 13.5" />
  </Icon>
);

export const SortIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M5 3v10M2.5 10.5L5 13l2.5-2.5M11 13V3M8.5 5.5L11 3l2.5 2.5" />
  </Icon>
);

export const CopyIcon = (p: Props) => (
  <Icon {...p}>
    <rect x="5.5" y="5.5" width="8" height="8" />
    <path d="M10.5 5.5V3.5c0-.6-.4-1-1-1h-6c-.6 0-1 .4-1 1v6c0 .6.4 1 1 1h2" />
  </Icon>
);

export const CheckIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M3.5 8.5l3 3 6-7" />
  </Icon>
);

export const CheckCircleIcon = (p: Props) => (
  <Icon {...p}>
    <circle cx="8" cy="8" r="6" />
    <path d="M5.5 8.2l1.7 1.7 3.3-3.6" />
  </Icon>
);

export const PersonIcon = (p: Props) => (
  <Icon {...p}>
    <circle cx="8" cy="5.5" r="2.5" />
    <path d="M3.5 13.5c.5-2.3 2.3-3.5 4.5-3.5s4 1.2 4.5 3.5" />
  </Icon>
);

export const CalendarIcon = (p: Props) => (
  <Icon {...p}>
    <rect x="2.5" y="3.5" width="11" height="10" />
    <path d="M2.5 6.5h11M5.5 2v3M10.5 2v3" />
  </Icon>
);

export const PencilIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M10.5 3l2.5 2.5L6 12.5l-3 .5.5-3z" />
  </Icon>
);

export const FinderIcon = (p: Props) => (
  <Icon {...p}>
    <path d="M9 3.5h3.5V7M12.5 3.5L7.5 8.5" />
    <path d="M10.5 9.5v3c0 .6-.4 1-1 1h-6c-.6 0-1-.4-1-1v-6c0-.6.4-1 1-1h3" />
  </Icon>
);
