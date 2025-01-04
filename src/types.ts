import { UserConfig as ViteConfig } from "vite";
import { z } from "zod";

export type FrontendBuildOutput = {
  kind: 'frontend';
  id: string;
  name: string;
  fileName: string;
  cssFileName: string | undefined;
  backendId: string | undefined;
}

export type BackendBuildOutput = {
    kind: 'backend';
    id: string;
    name: string;
    fileName: string;
}

export type BuildOutput = FrontendBuildOutput | BackendBuildOutput;

export const backendReferenceConfigSchema = z.strictObject({
  id: z.string(),
});

const viteSchema: z.ZodType<ViteConfig> = z.record(z.string(), z.unknown());

export const frontendPluginConfigSchema = z.strictObject({
  kind: z.literal('frontend'),
  id: z.string(),
  name: z.string().optional(),
  root: z.string(),
  backend: backendReferenceConfigSchema.nullable().optional(),
  vite: viteSchema.optional(),
});

export const backendPluginConfigSchema = z.strictObject({
  kind: z.literal('backend'),
  id: z.string(),
  name: z.string().optional(),
  root: z.string(),
});

export const workflowPluginConfigSchema = z.strictObject({
  kind: z.literal('workflow'),
  id: z.string(),
  name: z.string(),
  root: z.string(),
  definition: z.string(),
});

export const watchConfigSchema = z.strictObject({
  port: z.number().optional(),
  host: z.string().optional(),
});

export const caidoConfigSchema = z.strictObject({
  id: z.string(),
  name: z.string(),
  description: z.string(),
  version: z.string().regex(/^\d+\.\d+\.\d+$/),
  author: z.object({
      name: z.string(),
      email: z.string().email().optional(),
      url: z.string().url().optional(),
  }),
  plugins: z.array(
    z.discriminatedUnion('kind', [
      frontendPluginConfigSchema,
      backendPluginConfigSchema,
      workflowPluginConfigSchema,
    ])
  ),
  watch: watchConfigSchema.optional(),
});

// Type inference
export type BackendReferenceConfig = z.infer<typeof backendReferenceConfigSchema>;
export type FrontendPluginConfig = z.infer<typeof frontendPluginConfigSchema>;
export type BackendPluginConfig = z.infer<typeof backendPluginConfigSchema>;
export type WorkflowPluginConfig = z.infer<typeof workflowPluginConfigSchema>;
export type WatchConfig = z.infer<typeof watchConfigSchema>;
export type CaidoConfig = z.infer<typeof caidoConfigSchema>; 