// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { StaticJSONSchema } from "./StaticJSONSchema";
import type { ToolCallConfig } from "./ToolCallConfig";
import type { VariantInfo } from "./VariantInfo";

export type FunctionConfigJson = {
  variants: { [key in string]?: VariantInfo };
  system_schema: StaticJSONSchema | null;
  user_schema: StaticJSONSchema | null;
  assistant_schema: StaticJSONSchema | null;
  output_schema: StaticJSONSchema;
  implicit_tool_call_config: ToolCallConfig;
  description: string | null;
};
