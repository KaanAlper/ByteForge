/** Yama şablonu kimliği → insan-okur etiket. */
export const PATCH_LABEL: Record<string, string> = {
  return_true: "return true",
  return_false: "return false",
  return_value: "return N",
  return_max_int: "return MAX_INT",
  return_one_float: "return 1.0f",
  nop: "NOP",
  ret: "RET",
};

/** Yama şablonu kimliği → ARM64 assembly karşılığı (önizleme için). */
export const PATCH_ASM: Record<string, string> = {
  return_true: "MOV W0, #1 ; RET",
  return_false: "MOV W0, #0 ; RET",
  return_value: "MOV W0, #N ; RET",
  return_max_int: "MOVZ/MOVK W0, #0x7FFFFFFF ; RET",
  return_one_float: "FMOV S0, #1.0 ; RET",
  nop: "NOP",
  ret: "RET",
};
