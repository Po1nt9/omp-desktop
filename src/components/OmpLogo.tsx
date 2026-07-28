import { IconOmpMark } from "@/components/icons";

/** Original OMP monogram used across product identity surfaces. */
export function OmpLogo({ size = 22 }: { size?: number }) {
  return <IconOmpMark size={size} className="omp-logo" title="OMP" />;
}
