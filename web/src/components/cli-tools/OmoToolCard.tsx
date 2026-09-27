"use client";

import { useState, useEffect } from "react";
import React from "react";
import { Card, Button, ModelSelectModal } from "@/shared/components";

interface Tool {
  name: string;
  description: string;
}

interface OmoSlot {
  name: string;
  model: string;
  reasoning: string;
  fallbacks: string[];
}

interface OmoStatus {
  installed: boolean;
  exists: boolean;
  configPath: string;
  agents: OmoSlot[];
  categories: OmoSlot[];
}

interface Message {
  type: "success" | "error";
  text: string;
}

interface OmoToolCardProps {
  tool: Tool;
  isExpanded: boolean;
  onToggle: () => void;
  baseUrl: string;
  hasActiveProviders: boolean;
  apiKeys: Array<{ id: string; key: string }>;
  activeProviders: Array<{ provider: string; [key: string]: any }>;
  cloudEnabled: boolean;
  initialStatus?: OmoStatus | null;
}

const ENDPOINT = "/api/cli-tools/omo-settings";

export default function OmoToolCard({
  tool,
  isExpanded,
  onToggle,
  baseUrl: _baseUrl,
  hasActiveProviders,
  apiKeys: _apiKeys,
  activeProviders,
  cloudEnabled: _cloudEnabled,
  initialStatus,
}: OmoToolCardProps): React.ReactNode {
  const [omoStatus, setOmoStatus] = useState<OmoStatus | null>(initialStatus || null);
  const [checking, setChecking] = useState<boolean>(false);
  const [restoring, setRestoring] = useState<boolean>(false);
  const [message, setMessage] = useState<Message | null>(null);
  const [modalOpen, setModalOpen] = useState<boolean>(false);
  const [modelAliases, setModelAliases] = useState<Record<string, string>>({});
  const [editingSlot, setEditingSlot] = useState<{ kind: "agents" | "categories"; name: string } | null>(null);

  const fetchModelAliases = async (): Promise<void> => {
    try {
      const res = await fetch("/api/models/alias");
      const data = await res.json();
      if (res.ok) setModelAliases(data.aliases || {});
    } catch {
      console.log("Error fetching model aliases:");
    }
  };

  useEffect(() => {
    if (isExpanded && !omoStatus) {
      checkStatus();
      fetchModelAliases();
    }
    if (isExpanded) fetchModelAliases();
  }, [isExpanded]);

  useEffect(() => {
    if (initialStatus) setOmoStatus(initialStatus);
  }, [initialStatus]);

  const checkStatus = async (): Promise<void> => {
    setChecking(true);
    try {
      const res = await fetch(ENDPOINT);
      const data = await res.json();
      setOmoStatus(data);
    } catch {
      setOmoStatus({ installed: false, exists: false, configPath: "", agents: [], categories: [] });
    } finally {
      setChecking(false);
    }
  };

  const getEditingModel = (): string => {
    if (!editingSlot || !omoStatus) return "";
    const slot = omoStatus[editingSlot.kind].find(s => s.name === editingSlot.name);
    return slot?.model || "";
  };

  const handleModelSelect = (model: { value: string }): void => {
    if (!editingSlot) return;
    setModalOpen(false);
    applyModel(editingSlot.kind, editingSlot.name, model.value);
  };

  const applyModel = async (kind: "agents" | "categories", name: string, model: string): Promise<void> => {
    setMessage(null);
    try {
      const patchKind = kind === "agents" ? "agent" : "category";
      const res = await fetch(ENDPOINT, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ kind: patchKind, name, model }),
      });
      const data = await res.json();
      if (res.ok) {
        setMessage({ type: "success", text: `Updated ${patchKind} "${name}" model.` });
        setOmoStatus(data);
      } else {
        setMessage({ type: "error", text: data.error || "Failed to update model" });
      }
    } catch (error) {
      setMessage({ type: "error", text: (error as Error).message });
    }
  };

  const handleReset = async (): Promise<void> => {
    setRestoring(true);
    setMessage(null);
    try {
      const res = await fetch(ENDPOINT, { method: "DELETE" });
      const data = await res.json();
      if (res.ok) {
        setMessage({ type: "success", text: "Settings reset successfully!" });
        setOmoStatus(data);
      } else {
        setMessage({ type: "error", text: data.error || "Failed to reset settings" });
      }
    } catch (error) {
      setMessage({ type: "error", text: (error as Error).message });
    } finally {
      setRestoring(false);
    }
  };

  const openModelPicker = (kind: "agents" | "categories", name: string): void => {
    setEditingSlot({ kind, name });
    setModalOpen(true);
  };

  return (
    <Card padding="xs" className="overflow-hidden">
      <div className="flex items-start justify-between gap-3 hover:cursor-pointer sm:items-center" onClick={onToggle}>
        <div className="flex min-w-0 items-center gap-3">
          <div className="size-8 flex items-center justify-center shrink-0">
            <span className="inline-flex items-center justify-center font-bold rounded-lg size-8 bg-pink-500/10 text-pink-500 text-[10px]">OMO</span>
          </div>
          <div className="min-w-0">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h3 className="font-medium text-sm">{tool.name}</h3>
              {!omoStatus?.exists && omoStatus?.installed && <span className="px-1.5 py-0.5 text-[10px] font-medium bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 rounded-full">No config</span>}
            </div>
            <p className="text-xs text-text-muted truncate">{tool.description}</p>
          </div>
        </div>
        <span className={`material-symbols-outlined text-text-muted text-[20px] transition-transform ${isExpanded ? "rotate-180" : ""}`}>expand_more</span>
      </div>

      {isExpanded && (
        <div className="mt-4 pt-4 border-t border-border flex flex-col gap-4">
          {checking && (
            <div className="flex items-center gap-2 text-text-muted">
              <span className="material-symbols-outlined animate-spin">progress_activity</span>
              <span>Checking OMO settings...</span>
            </div>
          )}

          {!checking && omoStatus && !omoStatus.exists && (
            <div className="flex flex-col gap-3 p-4 bg-yellow-500/10 border border-yellow-500/30 rounded-lg">
              <div className="flex items-start gap-3">
                <span className="material-symbols-outlined text-yellow-500">warning</span>
                <div className="flex-1">
                  <p className="font-medium text-yellow-600 dark:text-yellow-400">OMO config not found</p>
                  <p className="text-sm text-text-muted mt-1">Create ~/.omo/omo.jsonc to configure sub-agent models.</p>
                </div>
              </div>
            </div>
          )}

          {omoStatus?.exists && (
            <>
              {(["agents", "categories"] as const).map((kind) => (
                <div key={kind} className="flex flex-col gap-2">
                  <h4 className="text-xs font-semibold text-text-main uppercase tracking-wider mt-2">
                    {kind === "agents" ? "Agents" : "Categories"}
                  </h4>
                  <div className="flex flex-col gap-1.5">
                    {omoStatus[kind].map((slot) => (
                      <div key={slot.name} className="flex flex-col gap-1.5">
                        <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-[8rem_auto_1fr_auto] sm:items-center sm:gap-2">
                          <span className="text-xs font-semibold text-text-main sm:text-right sm:text-sm pt-1.5">{slot.name}</span>
                          <span className="material-symbols-outlined hidden text-text-muted text-[14px] sm:inline mt-1.5">arrow_forward</span>
                          <div className="flex min-w-0 items-center gap-2">
                            <span className="min-w-0 truncate rounded bg-surface/40 px-2 py-2 text-xs text-text-muted sm:py-1.5">
                              {slot.model || "—"}
                            </span>
                            {slot.reasoning && (
                              <span className="shrink-0 px-1.5 py-0.5 text-[10px] font-medium bg-blue-500/10 text-blue-600 dark:text-blue-400 rounded-full">{slot.reasoning}</span>
                            )}
                          </div>
                          <button
                            onClick={() => openModelPicker(kind, slot.name)}
                            disabled={!hasActiveProviders}
                            className={`w-full sm:w-auto rounded border px-2 py-2 text-xs transition-colors sm:py-1.5 whitespace-nowrap sm:shrink-0 ${hasActiveProviders ? "bg-surface border-border text-text-main hover:border-primary cursor-pointer" : "opacity-50 cursor-not-allowed border-border"}`}
                          >
                            Select
                          </button>
                        </div>
                        {slot.fallbacks.length > 0 && (
                          <div className="sm:col-start-2 flex flex-wrap gap-1 mt-0.5 mb-1">
                            {slot.fallbacks.map((fb) => (
                              <span key={fb} className="inline-flex items-center px-1.5 py-0.5 text-[10px] font-medium bg-surface/60 text-text-muted rounded-full border border-border/50">{fb}</span>
                            ))}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              ))}

              {message && (
                <div className={`flex items-center gap-2 px-2 py-1.5 rounded text-xs ${message.type === "success" ? "bg-green-500/10 text-green-600" : "bg-red-500/10 text-red-600"}`}>
                  <span className="material-symbols-outlined text-[14px]">{message.type === "success" ? "check_circle" : "error"}</span>
                  <span>{message.text}</span>
                </div>
              )}

              <div className="grid grid-cols-1 gap-2 sm:flex sm:items-center">
                <Button variant="primary" size="sm" onClick={() => void handleReset()} disabled={restoring} loading={restoring}>
                  <span className="material-symbols-outlined text-[14px] mr-1">restore</span>Reset
                </Button>
              </div>
            </>
          )}
        </div>
      )}

      <ModelSelectModal
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        onSelect={handleModelSelect}
        selectedModel={getEditingModel()}
        activeProviders={activeProviders}
        modelAliases={modelAliases}
        title={`Select Model for ${editingSlot?.kind ?? ""} "${editingSlot?.name ?? ""}"`}
      />
    </Card>
  );
}
