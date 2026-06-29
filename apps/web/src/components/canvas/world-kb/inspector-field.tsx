/**
 * World KB inspector field wrapper (V1.74 A6 split).
 *
 * Reusable label + control + error layout used by the entity and relationship
 * inspectors. Kept minimal so the inspector components stay under the split cap.
 */
import { Label } from '@/components/ui/label';

interface FieldProps {
  label: string;
  htmlFor?: string;
  error?: string;
  children: React.ReactNode;
}

export function Field({ label, htmlFor, error, children }: FieldProps) {
  return (
    <div className="flex flex-col gap-1">
      <Label htmlFor={htmlFor} className="text-copy-13 text-gray-700">{label}</Label>
      {children}
      {error && <span className="text-copy-12 text-red-700">{error}</span>}
    </div>
  );
}
