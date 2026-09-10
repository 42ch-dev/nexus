import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';

import { filterGalleryEntries, type GalleryEntry } from '@/lib/gallery-index';

export type SectionIndexProps = {
  entries: readonly GalleryEntry[];
  onNavigate: (entry: GalleryEntry) => void;
};

export function SectionIndex({ entries, onNavigate }: SectionIndexProps) {
  const inputId = useId();
  const listId = useId();
  const statusId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState<number | null>(null);

  const filtered = useMemo(() => filterGalleryEntries(entries, query), [entries, query]);

  useEffect(() => {
    setActiveIndex(filtered.length > 0 ? 0 : null);
  }, [filtered]);

  const announce =
    filtered.length === 0
      ? query.trim()
        ? 'No matching sections.'
        : `${entries.length} sections.`
      : `${filtered.length} matching section${filtered.length === 1 ? '' : 's'}.`;

  function selectEntry(index: number) {
    const entry = filtered[index];
    if (!entry) return;
    onNavigate(entry);
  }

  function handleInputKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (filtered.length === 0) return;
      setActiveIndex((current) => current ?? 0);
      listRef.current?.querySelector<HTMLAnchorElement>('a[data-index="0"]')?.focus();
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (filtered.length === 0) return;
      const last = filtered.length - 1;
      setActiveIndex(last);
      listRef.current?.querySelector<HTMLAnchorElement>(`a[data-index="${last}"]`)?.focus();
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      if (activeIndex !== null) {
        selectEntry(activeIndex);
        return;
      }
      if (filtered.length > 0) selectEntry(0);
      return;
    }

    if (event.key === 'Escape') {
      event.preventDefault();
      setQuery('');
      setActiveIndex(null);
      inputRef.current?.focus();
    }
  }

  function handleLinkKeyDown(event: KeyboardEvent<HTMLAnchorElement>, index: number) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (index >= filtered.length - 1) return;
      const next = index + 1;
      setActiveIndex(next);
      listRef.current?.querySelector<HTMLAnchorElement>(`a[data-index="${next}"]`)?.focus();
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (index <= 0) {
        setActiveIndex(null);
        inputRef.current?.focus();
        return;
      }
      const prev = index - 1;
      setActiveIndex(prev);
      listRef.current?.querySelector<HTMLAnchorElement>(`a[data-index="${prev}"]`)?.focus();
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      selectEntry(index);
      return;
    }

    if (event.key === 'Escape') {
      event.preventDefault();
      setQuery('');
      setActiveIndex(null);
      inputRef.current?.focus();
    }
  }

  return (
    <section aria-label="Section filter" className="mb-6 rounded-card border border-gray-alpha-200 bg-background-100 p-4">
      <div className="flex flex-wrap items-end gap-3 mb-3">
        <div className="min-w-[min(100%,16rem)] flex-1">
          <label htmlFor={inputId} className="block text-label-14 font-medium text-gray-1000 mb-1">
            Filter sections
          </label>
          <input
            ref={inputRef}
            id={inputId}
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleInputKeyDown}
            placeholder="Search label, id, or keywords"
            className="w-full rounded-control border border-gray-alpha-300 bg-background-100 px-3 py-2 text-copy-14 text-gray-1000"
            aria-controls={listId}
            aria-describedby={statusId}
          />
        </div>
        {query.trim() ? (
          <button
            type="button"
            onClick={() => {
              setQuery('');
              setActiveIndex(null);
              inputRef.current?.focus();
            }}
            className="rounded-control border border-gray-alpha-300 px-3 py-2 text-label-14 text-gray-900 hover:bg-gray-alpha-100"
          >
            Clear
          </button>
        ) : null}
      </div>

      <p id={statusId} className="sr-only" aria-live="polite">
        {announce}
      </p>
      <p aria-hidden="true" className="text-copy-13 text-gray-700 mb-2">
        {filtered.length === 0
          ? query.trim()
            ? 'No matching sections.'
            : `${entries.length} sections`
          : `${filtered.length} matching section${filtered.length === 1 ? '' : 's'}`}
      </p>

      {filtered.length === 0 ? (
        query.trim() ? (
          <p className="text-copy-14 text-gray-700">No results for “{query.trim()}”.</p>
        ) : null
      ) : null}

      <ul
        id={listId}
        ref={listRef}
        className="flex flex-col gap-1"
        hidden={filtered.length === 0}
      >
        {filtered.map((entry, index) => (
          <li key={`${entry.path}#${entry.id}`}>
            <a
              href={`${entry.path}#${entry.id}`}
              data-index={index}
              tabIndex={0}
              aria-current={activeIndex === index ? 'true' : undefined}
              className="block rounded-control px-3 py-2 text-label-14 text-gray-900 no-underline hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
              onClick={(event) => {
                event.preventDefault();
                selectEntry(index);
              }}
              onFocus={() => setActiveIndex(index)}
              onKeyDown={(event) => handleLinkKeyDown(event, index)}
            >
              {entry.label}
            </a>
          </li>
        ))}
      </ul>
    </section>
  );
}
