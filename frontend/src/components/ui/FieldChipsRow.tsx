import { useFieldValues } from '../../hooks/useFieldValues';
import { FieldChip } from './FieldChip';

interface FieldDef {
  field: string;
  label?: string;
  freeText?: boolean;
}

interface Props {
  fields?: FieldDef[];
  onInsert: (token: string) => void;
}

const DEFAULT_FIELDS: FieldDef[] = [
  { field: 'state' },
  { field: 'branch' },
  { field: 'repo' },
  { field: 'stage' },
  { field: 'exit_code', label: 'exit_code' },
  { field: 'bucket' },
];

export function FieldChipsRow({ fields = DEFAULT_FIELDS, onInsert }: Props) {
  const fieldValues = useFieldValues();

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="text-xs text-disabled mr-0.5">Fields:</span>
      {fields.map((def) => {
        const values = fieldValues[def.field] ?? [];
        return (
          <FieldChip
            key={def.field}
            field={def.field}
            label={def.label}
            values={values}
            onInsert={onInsert}
            freeText={def.freeText}
          />
        );
      })}
    </div>
  );
}
