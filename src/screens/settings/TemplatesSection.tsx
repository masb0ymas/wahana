import { useContext, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { confirm } from "@/components/Confirm";
import { Button, Input, Label } from "@/components/ui";
import { errMsg } from "@/lib/utils";
import {
  deleteTemplate,
  fillTemplate,
  listTemplates,
  saveTemplate,
  TEMPLATE_VARIABLES,
  templateError,
  type Template,
} from "@/store/templates";
import { ScopeCtx } from "./shared";

/** Sample values for the editor's preview. */
const SAMPLE = { name: "Budi", phone: "+62 812-3456-7890" };

export function TemplatesSection() {
  const { scope } = useContext(ScopeCtx);
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["templates", scope], queryFn: () => listTemplates(scope), enabled: !!scope });
  const [editing, setEditing] = useState<Partial<Template> | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["templates"] });

  if (!scope) {
    return <p className="text-sm text-neutral-500">Connect a WhatsApp account first to manage templates.</p>;
  }

  return (
    <>
      <ul className="space-y-1.5">
        {q.data?.map((t) => (
          <li key={t.id} className="flex items-start gap-2 rounded-lg bg-neutral-50 dark:bg-neutral-800/60 px-3 py-2">
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-medium">
                {t.name}
                {t.uses > 0 && <span className="ml-2 text-[11px] font-normal text-neutral-400">used {t.uses}×</span>}
              </span>
              <span className="block text-xs text-neutral-500 whitespace-pre-wrap line-clamp-3 selectable">{t.text}</span>
            </span>
            <button
              className="text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
              onClick={() => setEditing(t)}
              title="Edit"
            >
              <Pencil size={14} />
            </button>
            <button
              className="text-neutral-400 hover:text-red-600"
              title="Delete"
              onClick={async () => {
                if (await confirm({ title: `Delete "${t.name}"?`, danger: true, confirmLabel: "Delete" })) {
                  await deleteTemplate(t.id);
                  await refresh();
                }
              }}
            >
              <Trash2 size={14} />
            </button>
          </li>
        ))}
        {q.data?.length === 0 && !editing && <li className="text-sm text-neutral-500">No templates for this account yet.</li>}
      </ul>
      {editing ? (
        <TemplateEditor
          key={editing.id ?? "new"}
          template={editing}
          onCancel={() => setEditing(null)}
          onSave={async (name, text) => {
            await saveTemplate({ id: editing.id ?? crypto.randomUUID(), account: scope, name, text });
            await refresh();
            setEditing(null);
          }}
        />
      ) : (
        <Button variant="secondary" onClick={() => setEditing({})}>
          <Plus size={14} /> Add template
        </Button>
      )}
    </>
  );
}

function TemplateEditor({
  template,
  onSave,
  onCancel,
}: {
  template: Partial<Template>;
  onSave: (name: string, text: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState(template.name ?? "");
  const [text, setText] = useState(template.text ?? "");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const textRef = useRef<HTMLTextAreaElement>(null);

  const insertVariable = (token: string) => {
    const ta = textRef.current;
    const start = ta?.selectionStart ?? text.length;
    const end = ta?.selectionEnd ?? text.length;
    setText(text.slice(0, start) + token + text.slice(end));
    requestAnimationFrame(() => {
      if (!ta) return;
      ta.focus();
      ta.selectionStart = ta.selectionEnd = start + token.length;
    });
  };

  const save = async () => {
    const problem = templateError(name, text);
    if (problem) return setErr(problem);
    setBusy(true);
    try {
      await onSave(name, text);
    } catch (e) {
      setErr(errMsg(e));
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3 rounded-lg border border-dashed border-neutral-300 dark:border-neutral-700 p-3">
      <div>
        <Label>Name</Label>
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Order confirmation" autoFocus />
      </div>
      <div>
        <Label>Message</Label>
        <textarea
          ref={textRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void save();
            }
          }}
          rows={5}
          placeholder="Halo {{name}}, pesanan kamu sudah kami terima dan sedang diproses."
          className="w-full rounded-lg border border-neutral-300 dark:border-neutral-700 bg-transparent px-3 py-2 text-sm outline-none focus:border-wa-dark"
        />
        <div className="mt-1 flex flex-wrap items-center gap-1">
          <span className="text-[11px] text-neutral-500">Insert:</span>
          {TEMPLATE_VARIABLES.map((v) => (
            <button
              key={v.token}
              onClick={() => insertVariable(v.token)}
              title={`${v.hint}, filled in when the template is used`}
              className="rounded-full bg-neutral-100 px-2 py-0.5 font-mono text-[11px] text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
            >
              {v.token}
            </button>
          ))}
        </div>
      </div>
      {text.trim() && (
        <div>
          <Label>Preview</Label>
          <div className="rounded-lg bg-wa/10 px-3 py-2 text-sm whitespace-pre-wrap">{fillTemplate(text, SAMPLE)}</div>
        </div>
      )}
      {err && <div className="text-xs text-red-600">{err}</div>}
      <div className="flex gap-2">
        <Button onClick={() => void save()} disabled={busy}>
          Save
        </Button>
        <Button variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
