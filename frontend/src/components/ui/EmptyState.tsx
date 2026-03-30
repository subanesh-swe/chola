interface Props {
  message: string;
  description?: string;
}

export function EmptyState({ message, description }: Props) {
  return (
    <div className="text-center py-12">
      <p className="text-lg text-slate-400">{message}</p>
      {description && <p className="text-sm text-slate-500 mt-2">{description}</p>}
    </div>
  );
}
