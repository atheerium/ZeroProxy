"use client";

import { useState, useEffect, useCallback } from "react";
import Card from "@/shared/components/Card";
import Button from "@/shared/components/Button";
import Drawer from "@/shared/components/Drawer";
import Pagination from "@/shared/components/Pagination";
import Badge from "@/shared/components/Badge";
import { cn } from "@/shared/utils/cn";
import { AI_PROVIDERS, getProviderByAlias } from "@/shared/constants/providers";

interface ProviderNameCache {
  [key: string]: string | { name: string };
}

interface ProviderNodesCache {
  [key: string]: string;
}

let providerNameCache: ProviderNameCache | null = null;
let providerNodesCache: ProviderNodesCache | null = null;

async function fetchProviderNames(): Promise<{ providerNameCache: ProviderNameCache; providerNodesCache: ProviderNodesCache }> {
  if (providerNameCache && providerNodesCache) {
    return { providerNameCache, providerNodesCache };
  }

  const nodesRes = await fetch("/api/provider-nodes");
  const nodesData = await nodesRes.json();
  const nodes = nodesData.nodes || [];
  providerNodesCache = {};

  for (const node of nodes) {
    providerNodesCache[node.id] = node.name;
  }

  providerNameCache = {
    ...AI_PROVIDERS,
    ...providerNodesCache
  };

  return { providerNameCache, providerNodesCache };
}

function getProviderName(providerId: string | undefined, cache: ProviderNameCache | null): string {
  if (!providerId) return providerId || "";
  if (!cache) return providerId;

  const cached = cache[providerId];

  if (typeof cached === 'string') {
    return cached;
  }

  if (cached?.name) {
    return cached.name;
  }

  const providerConfig = getProviderByAlias(providerId) || AI_PROVIDERS[providerId];
  return providerConfig?.name || providerId;
}

function getInputTokens(tokens: Tokens | undefined): number {
  const prompt = tokens?.prompt_tokens || tokens?.input_tokens || 0;
  const cache = tokens?.cached_tokens || tokens?.cache_read_input_tokens || 0;
  return prompt < cache ? cache : prompt;
}

interface Tokens {
  prompt_tokens?: number;
  input_tokens?: number;
  cached_tokens?: number;
  cache_read_input_tokens?: number;
  completion_tokens?: number;
  output_tokens?: number;
}

interface Latency {
  ttft?: number;
  total?: number;
}

interface RequestDetail {
  id: string;
  timestamp: string;
  model: string;
  provider: string;
  tokens?: Tokens;
  latency?: Latency;
  status: string;
}

interface PaginationState {
  page: number;
  pageSize: number;
  totalItems: number;
  totalPages: number;
}

interface FilterState {
  provider: string;
  startDate: string;
  endDate: string;
}

interface Provider {
  id: string;
  name: string;
}

export default function RequestHistoryTable({
  showFilters = true,
  pageSize = 20,
}: {
  showFilters?: boolean;
  pageSize?: number;
}) {
  const [details, setDetails] = useState<RequestDetail[]>([]);
  const [pagination, setPagination] = useState<PaginationState>({
    page: 1,
    pageSize,
    totalItems: 0,
    totalPages: 0
  });
  const [loading, setLoading] = useState<boolean>(false);
  const [selectedDetail, setSelectedDetail] = useState<RequestDetail | null>(null);
  const [isDrawerOpen, setIsDrawerOpen] = useState<boolean>(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [providerNameCacheLocal, setProviderNameCacheLocal] = useState<ProviderNameCache | null>(null);
  const [filters, setFilters] = useState<FilterState>({
    provider: "",
    startDate: "",
    endDate: ""
  });

  const fetchProviders = useCallback(async () => {
    try {
      const res = await fetch("/api/usage/providers");
      const data = await res.json();
      setProviders(data.providers || []);

      const cache = await fetchProviderNames();
      setProviderNameCacheLocal(cache.providerNameCache);
    } catch (error) {
      console.error("Failed to fetch providers:", error);
    }
  }, []);

  const fetchDetails = useCallback(async () => {
    setLoading(true);
    try {
      const params = new URLSearchParams({
        page: pagination.page.toString(),
        pageSize: pagination.pageSize.toString()
      });
      if (filters.provider) params.append("provider", filters.provider);
      if (filters.startDate) params.append("startDate", filters.startDate);
      if (filters.endDate) params.append("endDate", filters.endDate);

      const res = await fetch(`/api/usage/request-details?${params}`);
      const data = await res.json();

      const mappedDetails = (data.details || []).map((detail: any) => {
        const isSuccess = !detail.status || detail.status === "ok" || detail.status === "success";

        return {
          ...detail,
          status: detail.status || (isSuccess ? "success" : "error")
        };
      });

      setDetails(mappedDetails);
      setPagination(prev => ({ ...prev, ...data.pagination }));
    } catch (error) {
      console.error("Failed to fetch request details:", error);
    } finally {
      setLoading(false);
    }
  }, [pagination.page, pagination.pageSize, filters]);

  useEffect(() => {
    fetchProviders();
  }, [fetchProviders]);

  useEffect(() => {
    fetchDetails();
  }, [fetchDetails]);

  const handleViewDetail = (detail: RequestDetail) => {
    setSelectedDetail(detail);
    setIsDrawerOpen(true);
  };

  const handlePageChange = (newPage: number) => {
    setPagination(prev => ({ ...prev, page: newPage }));
  };

  const handlePageSizeChange = (newPageSize: number) => {
    setPagination(prev => ({ ...prev, pageSize: newPageSize, page: 1 }));
  };

  const handleClearFilters = () => {
    setFilters({ provider: "", startDate: "", endDate: "" });
  };

  const isSuccess = (status: string) => !status || status === "ok" || status === "success";

  return (
    <div className="flex min-w-0 flex-col gap-6">
      {showFilters && (
        <Card padding="md">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <div className="flex min-w-0 flex-col gap-2">
              <label htmlFor="provider-filter" className="text-sm font-medium text-text-main">Provider</label>
              <select
                id="provider-filter"
                value={filters.provider}
                onChange={(e: React.ChangeEvent<HTMLSelectElement>) => setFilters({ ...filters, provider: e.target.value })}
                className={cn(
                  "h-9 px-3 rounded-lg border border-black/10 dark:border-white/10 bg-surface",
                  "text-sm text-text-main focus:outline-none focus:ring-2 focus:ring-primary/20",
                  "w-full min-w-0 cursor-pointer"
                )}
              >
                <option value="">All Providers</option>
                {providers.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.name}
                  </option>
                ))}
              </select>
            </div>
            
            <div className="flex min-w-0 flex-col gap-2">
              <label htmlFor="start-date-filter" className="text-sm font-medium text-text-main">Start Date</label>
              <input
                id="start-date-filter"
                type="datetime-local"
                value={filters.startDate}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => setFilters({ ...filters, startDate: e.target.value })}
                className={cn(
                  "h-9 px-3 rounded-lg border border-black/10 dark:border-white/10 bg-surface",
                  "w-full min-w-0 text-sm text-text-main focus:outline-none focus:ring-2 focus:ring-primary/20"
                )}
              />
            </div>

            <div className="flex min-w-0 flex-col gap-2">
              <label htmlFor="end-date-filter" className="text-sm font-medium text-text-main">End Date</label>
              <input
                id="end-date-filter"
                type="datetime-local"
                value={filters.endDate}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => setFilters({ ...filters, endDate: e.target.value })}
                className={cn(
                  "h-9 px-3 rounded-lg border border-black/10 dark:border-white/10 bg-surface",
                  "w-full min-w-0 text-sm text-text-main focus:outline-none focus:ring-2 focus:ring-primary/20"
                )}
              />
            </div>
            
            <div className="flex min-w-0 flex-col gap-2 sm:col-span-2 lg:col-span-1">
              <span className="hidden text-sm font-medium text-text-main opacity-0 lg:block" aria-hidden="true">Clear</span>
              <Button 
                variant="ghost" 
                onClick={handleClearFilters}
                disabled={!filters.provider && !filters.startDate && !filters.endDate}
                className="w-full"
              >
                Clear Filters
              </Button>
            </div>
          </div>
        </Card>
      )}

      <Card padding="none">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[880px]">
            <thead>
              <tr className="border-b border-black/5 dark:border-white/5">
                <th className="text-left p-4 text-sm font-semibold text-text-main">Timestamp</th>
                <th className="text-left p-4 text-sm font-semibold text-text-main">Model</th>
                <th className="text-left p-4 text-sm font-semibold text-text-main">Provider</th>
                <th className="text-right p-4 text-sm font-semibold text-text-main">Input Tokens</th>
                <th className="text-right p-4 text-sm font-semibold text-text-main">Output Tokens</th>
                <th className="text-right p-4 text-sm font-semibold text-text-main">TTFT</th>
                <th className="text-right p-4 text-sm font-semibold text-text-main">Total Latency</th>
                <th className="text-left p-4 text-sm font-semibold text-text-main">Status</th>
                <th className="text-center p-4 text-sm font-semibold text-text-main">Action</th>
              </tr>
            </thead>
            <tbody>
              {loading ? (
                <tr>
                  <td colSpan={9} className="p-8 text-center text-text-muted">
                    <div className="flex items-center justify-center gap-2">
                      <span className="material-symbols-outlined animate-spin text-[20px]">progress_activity</span>
                      Loading...
                    </div>
                  </td>
                </tr>
              ) : details.length === 0 ? (
                <tr>
                  <td colSpan={9} className="p-8 text-center text-text-muted">
                    No request details found
                  </td>
                </tr>
              ) : (
                details.map((detail, index) => (
                  <tr
                    key={`${detail.id}-${index}`}
                    className={
                      `border-b border-black/5 dark:border-white/5 last:border-b-0 hover:bg-black/[0.02] dark:hover:bg-white/[0.02] transition-colors ` +
                      (isSuccess(detail.status) ? "" : "bg-red-50 dark:bg-red-950/20")
                    }
                  >
                    <td className="whitespace-nowrap p-4 text-sm text-text-main">
                      {new Date(detail.timestamp).toLocaleString()}
                    </td>
                    <td className="max-w-[260px] truncate p-4 font-mono text-sm text-text-main">
                      {detail.model}
                    </td>
                    <td className="max-w-[180px] truncate p-4 text-sm text-text-main">
                      <span className="font-medium">
                        {getProviderName(detail.provider, providerNameCacheLocal)}
                      </span>
                    </td>
                    <td className="p-4 text-sm text-text-main text-right font-mono">
                      {getInputTokens(detail.tokens).toLocaleString()}
                    </td>
                    <td className="p-4 text-sm text-text-main text-right font-mono">
                      {(detail.tokens?.completion_tokens || detail.tokens?.output_tokens || 0).toLocaleString()}
                    </td>
                    <td className="p-4 text-sm text-text-muted text-right font-mono">
                      {detail.latency?.ttft || 0}ms
                    </td>
                    <td className="p-4 text-sm text-text-muted text-right font-mono">
                      {detail.latency?.total || 0}ms
                    </td>
                    <td className="p-4 text-sm">
                      <Badge 
                        variant={isSuccess(detail.status) ? "success" : "error"} 
                        size="sm"
                      >
                        {detail.status}
                      </Badge>
                    </td>
                    <td className="p-4 text-center">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => handleViewDetail(detail)}
                      >
                        Detail
                      </Button>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>

        {!loading && details.length > 0 && (
          <div className="border-t border-black/5 dark:border-white/5">
            <Pagination
              currentPage={pagination.page}
              pageSize={pagination.pageSize}
              totalItems={pagination.totalItems}
              onPageChange={handlePageChange}
              onPageSizeChange={handlePageSizeChange}
            />
          </div>
        )}
      </Card>

      <Drawer
        isOpen={isDrawerOpen}
        onClose={() => setIsDrawerOpen(false)}
        title="Request Details"
        width="lg"
      >
        {selectedDetail && (
          <div className="space-y-6">
            <div className="grid min-w-0 grid-cols-1 gap-4 text-sm sm:grid-cols-2">
              <div>
                <span className="text-text-muted">ID:</span>{" "}
                <span className="break-all font-mono text-text-main">{selectedDetail.id}</span>
              </div>
              <div>
                <span className="text-text-muted">Timestamp:</span>{" "}
                <span className="text-text-main">{new Date(selectedDetail.timestamp).toLocaleString()}</span>
              </div>
              <div>
                 <span className="text-text-muted">Provider:</span>{" "}
                 <span className="text-text-main font-medium">{getProviderName(selectedDetail.provider, providerNameCacheLocal)}</span>
              </div>
              <div>
                <span className="text-text-muted">Model:</span>{" "}
                <span className="text-text-main font-mono">{selectedDetail.model}</span>
              </div>
              <div>
                <span className="text-text-muted">Status:</span>{" "}
                <span className={cn(
                  "font-medium",
                  isSuccess(selectedDetail.status) ? "text-green-600" : "text-red-600"
                )}>
                  {selectedDetail.status}
                </span>
              </div>
              <div>
                <span className="text-text-muted">Latency:</span>{" "}
                <span className="text-text-main font-mono">
                  TTFT {selectedDetail.latency?.ttft || 0}ms / Total {selectedDetail.latency?.total || 0}ms
                </span>
              </div>
              <div>
                <span className="text-text-muted">Input Tokens:</span>{" "}
                <span className="text-text-main font-mono">
                  {getInputTokens(selectedDetail.tokens).toLocaleString()}
                </span>
              </div>
              <div>
                <span className="text-text-muted">Output Tokens:</span>{" "}
                <span className="text-text-main font-mono">
                  {(selectedDetail.tokens?.completion_tokens || selectedDetail.tokens?.output_tokens || 0).toLocaleString()}
                </span>
              </div>
            </div>
            
            <div className="space-y-4">
              <CollapsibleSection title="1. Client Request (Input)" defaultOpen={true} icon="input">
                <pre className="max-h-[300px] max-w-full overflow-auto rounded-lg border border-black/5 bg-black/5 p-3 font-mono text-xs text-text-main dark:border-white/5 dark:bg-white/5 sm:p-4">
                  {JSON.stringify(selectedDetail, null, 2)}
                </pre>
              </CollapsibleSection>
            </div>
          </div>
        )}
      </Drawer>
    </div>
  );
}

function CollapsibleSection({ title, children, defaultOpen = false, icon = null }: {
  title: string;
  children: React.ReactNode;
  defaultOpen?: boolean;
  icon?: string;
}) {
  const [isOpen, setIsOpen] = useState<boolean>(defaultOpen);
  
  return (
    <div className="border border-black/5 dark:border-white/5 rounded-lg overflow-hidden">
      <button 
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center justify-between p-3 bg-black/[0.02] dark:bg-white/[0.02] hover:bg-black/[0.04] dark:hover:bg-white/[0.04] transition-colors"
      >
        <div className="flex items-center gap-2">
          {icon && <span className="material-symbols-outlined text-[18px] text-text-muted">{icon}</span>}
          <span className="font-semibold text-sm text-text-main">{title}</span>
        </div>
        <span className={cn(
          "material-symbols-outlined text-[20px] text-text-muted transition-transform duration-200",
          isOpen ? "rotate-90" : ""
        )}>
          chevron_right
        </span>
      </button>
      
      {isOpen && (
        <div className="p-4 border-t border-black/5 dark:border-white/5">
          {children}
        </div>
      )}
    </div>
  );
}