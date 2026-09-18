export type RuntimeKind =
  | "unity_il2cpp"
  | "unity_mono"
  | "flutter"
  | "react_native"
  | "java_kotlin"
  | "unknown";

export interface SignatureSchemes {
  v1: boolean;
  v2: boolean;
  v3: boolean;
}

export interface ManifestInfo {
  package: string | null;
  version_code: string | null;
  version_name: string | null;
  min_sdk: string | null;
  target_sdk: string | null;
  debuggable: boolean;
  permissions: string[];
  launchable_activities: string[];
}

export interface PackerHit {
  name: string;
  vendor: string;
  confidence: string;
  evidence: string;
}

export interface AppProfile {
  file_name: string;
  entry_count: number;
  runtime: RuntimeKind;
  abis: string[];
  signatures: SignatureSchemes;
  manifest: ManifestInfo | null;
  recommended_pipeline: string;
  packers: PackerHit[];
}

export interface ArchiveDiff {
  sha256_a: string;
  sha256_b: string;
  added: string[];
  removed: string[];
  modified: string[];
  unchanged: number;
}

export interface ElfSegment {
  index: number;
  vaddr: number;
  file_offset: number;
  file_size: number;
  mem_size: number;
  flags: string;
}

export interface SoSymbol {
  name: string;
  rva: number;
  file_offset: number | null;
  size: number;
  is_function: boolean;
  /** Bilinen bir yama şablonuyla eşleşiyorsa kimliği (ör. "return_true"); yoksa null. */
  patched: string | null;
}

export type PatchTemplate =
  | { kind: "return_true" }
  | { kind: "return_false" }
  | { kind: "return_max_int" }
  | { kind: "return_one_float" }
  | { kind: "nop" }
  | { kind: "ret" }
  | { kind: "return_value"; value: number };

export interface PatchPreview {
  offset: number;
  current: string;
  replacement: string;
}

export interface ResignResult {
  signed_path: string;
  included_mods: boolean;
  mod_count: number;
  note: string;
}

export interface AdbDevice {
  serial: string;
  state: string;
}

export interface SmaliMethod {
  signature: string;
  return_type: string;
  start_line: number;
  end_line: number;
}

export interface SmaliMatch {
  file: string;
  line: number;
  text: string;
}

export interface SmaliRuleHit {
  file: string;
  rule: string;
  category: string;
  method: string | null;
  line: number;
  snippet: string;
  suggestion: string;
}

export interface Score {
  name: string;
  score: number;
  reasons: string[];
  confidence: string;
  excluded: boolean;
  category: string;
  suggested_template: string;
  risky: boolean;
}

export interface RankedTarget {
  name: string;
  type_name: string;
  method_name: string;
  image: string;
  rva: number;
  score: number;
  reasons: string[];
  confidence: string;
  category: string;
  suggested_template: string;
  risky: boolean;
}

/** Sezgisel hedef satırı — çözücü varsa rva/type_name/image dolu gelir.
 *  Hem rank_symbols (Score) hem il2cpp_rank_resolved (RankedTarget) buna atanabilir. */
export interface TargetRow {
  name: string;
  score: number;
  category: string;
  risky: boolean;
  reasons?: string[];
  confidence?: string;
  suggested_template?: string;
  rva?: number;
  type_name?: string;
  image?: string;
}
export interface SmaliTarget {
  file: string;
  signature: string;
  return_type: string;
  score: number;
  confidence: string;
  reasons: string[];
  patchable: boolean;
  category: string;
  suggested_template: string;
  risky: boolean;
}

export type PrologueCheck =
  | { status: "prologue"; detail: string }
  | { status: "already_patched"; template: string }
  | { status: "not_prologue"; warning: string };

export interface DecodeResult {
  out_dir: string;
  smali_files: number;
}

export type StreamEvent =
  | { type: "line"; text: string }
  | { type: "progress"; percent: number; current: number; total: number }
  | { type: "done"; out_dir: string; files: number }
  | { type: "error"; message: string };

export interface CacheStatus {
  cached: boolean;
  out_dir: string;
  files: number;
}

export type FeatureTarget =
  | { kind: "smali_boolean"; class: string; method: string; ret: boolean }
  | { kind: "il2cpp_rva"; offset: number; template: string }
  | { kind: "native_offset"; offset: number; template: string };

export interface MenuFeature {
  name: string;
  target: FeatureTarget;
}

export interface MenuConfig {
  title: string;
  features: MenuFeature[];
}

export interface InjectResult {
  out_apk: string;
  steps: string[];
  deferred_native: string[];
}

export type MemValueType = "i32" | "i64" | "f32" | "f64" | "u8";

export interface ProcInfo {
  pid: number;
  name: string;
  icon?: string | null;
}

export type Compare =
  | "exact"
  | "not_equal"
  | "greater"
  | "less"
  | "between"
  | "unknown"
  | "increased"
  | "decreased"
  | "changed"
  | "unchanged"
  | "increased_by"
  | "decreased_by";

export interface AddrValue {
  address: number;
  value: string;
}

export interface ScanSummary {
  session: number;
  total: number;
  truncated: boolean;
  scanned_regions: number;
  preview: AddrValue[];
}

export interface FrozenInfo {
  pid: number;
  address: number;
  value: string;
  label: string;
  ty: MemValueType;
}

export interface PointerChain {
  base_module: string;
  base_address: number;
  base_offset: number;
  offsets: number[];
  depth: number;
}

/** Kayıtlı adres tablosundaki tek giriş (frontend). */
export interface SavedEntry {
  id: string;
  pid: number;
  address: number;
  ty: MemValueType;
  label: string;
  value: string;
  frozen: boolean;
}

export interface Finding {
  severity: string;
  title: string;
  detail: string;
  entries: string[];
  category: string;
}

export interface ModPatch {
  offset: number;
  rva: number | null;
  old_hex: string;
  new_hex: string;
  len: number;
  template: string | null;
  label: string;
  symbol: string | null;
}

export interface CheckpointInfo {
  name: string;
  path: string;
}

export interface JadxResult {
  out_dir: string;
  java_files: number;
}

export interface JavaMatch {
  file: string;
  line: number;
  text: string;
}

export type ForcedReturn =
  | { kind: "true" }
  | { kind: "false" }
  | { kind: "void" }
  | { kind: "null" }
  | { kind: "max_int" };

export interface YaraMatch {
  rule: string;
  tags: string[];
  description: string;
  hit_count: number;
}

export interface Decompiled {
  signature: string;
  pseudocode: string;
  note: string;
  complex: boolean;
}

export interface DisasmLine {
  address: number;
  bytes: string;
  text: string;
}

export interface HexChunk {
  total: number;
  offset: number;
  bytes: number[];
}

export interface SplitResult {
  out_dir: string;
  apks: string[];
}

export interface Il2CppResult {
  out_dir: string;
  metadata_valid: boolean;
  metadata_version: number;
  metadata_size: number;
  has_binary: boolean;
  binary_arch: string[];
  note: string;
}

export interface DumpResult {
  count: number;
  symbols: string[];
  out_file: string;
  metadata_version: number;
}

export interface ResolvedMethod {
  name: string;
  type_name: string;
  method_name: string;
  rva: number;
  image: string;
}

export interface ResolveSummary {
  resolved: number;
  total_methods: number;
  images_resolved: number;
  metadata_version: number;
  out_file: string;
  note: string;
}

export interface PatchRecord {
  id: number;
  ts: number;
  kind: string;
  path: string;
  offset: number;
  old_hex: string;
  new_hex: string;
  note: string;
  reverted: boolean;
}

export interface ToolStatus {
  name: string;
  available: boolean;
  detail: string;
}

export interface ToolReport {
  tools: ToolStatus[];
}

export type ApiErrorKind =
  | "invalid_path"
  | "not_a_file"
  | "too_large"
  | "bad_extension"
  | "io"
  | "archive"
  | "manifest"
  | "entry_too_large"
  | "unknown_format";

export interface ApiError {
  kind: ApiErrorKind;
  message: string;
}

export interface PeProfile {
  is_64bit: boolean;
  is_dll: boolean;
  machine: string;
  subsystem: string;
  is_dotnet: boolean;
  compiler: string;
  sections: string[];
  imported_dlls: string[];
  entry_point: number;
  recommended_pipeline: string;
  packers: PackerHit[];
  section_entropy: SectionEntropy[];
  likely_packed: boolean;
}

export interface SectionEntropy {
  name: string;
  bits: number;
  class: "low" | "normal" | "high";
}

export interface PeSection {
  name: string;
  virtual_address: number;
  virtual_size: number;
  raw_pointer: number;
  raw_size: number;
  flags: string;
}

export interface PeExport {
  name: string;
  rva: number;
  file_offset: number | null;
  patched: string | null;
}

export interface TamperHit {
  name: string;
  category: string;
  note: string;
}
