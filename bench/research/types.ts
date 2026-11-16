export interface Checks {
  facts: string[][];
  quotes: string[][];
  analysis: string[];
  forbidden: string[];
  minSources: number;
  rubric: string;
}
export interface ResearchCase {
  id: string;
  family: string;
  online: boolean;
  topic: string;
  documents: { name: string; text: string }[];
  pages: { url: string; title: string; text: string; snippet?: string }[];
  search_error?: string;
  checks: Checks;
  restart?: { question: string; checks: Checks };
}
export interface Corpus { version: 1; split: 'smoke' | 'held-out'; cases: ResearchCase[] }
export interface Evidence { source: string; title: string; location: string; text: string }
export interface ToolEvent { type?: string; name?: string; success?: boolean; output?: string; arguments?: unknown }
export interface Turn {
  answer: string; error: string | null; seconds: number;
  events: ToolEvent[]; traces?: unknown[]; generations?: Record<string, unknown>[];
  evidence: { sources: Evidence[]; network: { kind: string; topic?: string; url?: string; offline: boolean }[] };
  first_visible_seconds?: number | null;
  adapter_seconds?: number;
}
export interface Row { id: string; model: string; side: 'pr3' | 'openjarvis'; seed: number; turns: Turn[]; error?: string }
