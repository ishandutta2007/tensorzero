// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { LLMJudgeIncludeConfig } from "./LLMJudgeIncludeConfig";
import type { LLMJudgeInputFormat } from "./LLMJudgeInputFormat";
import type { LLMJudgeOptimize } from "./LLMJudgeOptimize";
import type { LLMJudgeOutputType } from "./LLMJudgeOutputType";

export type LLMJudgeConfig = {
  input_format: LLMJudgeInputFormat;
  output_type: LLMJudgeOutputType;
  include: LLMJudgeIncludeConfig;
  optimize: LLMJudgeOptimize;
  cutoff: number | null;
};
