import type { ReactNode } from "react";
import { ThemeIcon } from "@mantine/core";

interface EmptyStateProps {
  icon: ReactNode;
  title: string;
  hint?: string;
  fill?: boolean;
}

export default function EmptyState({ icon, title, hint, fill }: EmptyStateProps) {
  return (
    <div className={`flex flex-col items-center justify-center gap-2 ${fill ? "h-full py-0" : "py-10"}`}>
      <ThemeIcon variant="light" color="gray" size="lg" radius="xl">
        {icon}
      </ThemeIcon>
      <div className="text-sm font-semibold text-white">{title}</div>
      {hint && <div className="text-xs text-(--mantine-color-dark-0)">{hint}</div>}
    </div>
  );
}
