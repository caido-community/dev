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

export const frontendPluginConfigSchema = z.strictObject({
  kind: z.literal('frontend'),
  name: z.string().optional(),
  root: z.string(),
  backend: backendReferenceConfigSchema.nullable().optional(),
});

export const backendPluginConfigSchema = z.strictObject({
  kind: z.literal('backend'),
  name: z.string().nullable().optional(),
  root: z.string(),
});

export const workflowPluginConfigSchema = z.strictObject({
  kind: z.literal('workflow'),
  id: z.string(),
  name: z.string(),
  root: z.string(),
  definition: z.string(),
});

export const devConfigSchema = z.strictObject({
  port: z.number().optional(),
  host: z.string().optional(),
});

export const caidoConfigSchema = z.strictObject({
  plugins: z.array(
    z.discriminatedUnion('kind', [
      frontendPluginConfigSchema,
      backendPluginConfigSchema,
      workflowPluginConfigSchema,
    ])
  ),
  dev: devConfigSchema.optional(),
});

// Type inference
export type BackendReferenceConfig = z.infer<typeof backendReferenceConfigSchema>;
export type FrontendPluginConfig = z.infer<typeof frontendPluginConfigSchema>;
export type BackendPluginConfig = z.infer<typeof backendPluginConfigSchema>;
export type WorkflowPluginConfig = z.infer<typeof workflowPluginConfigSchema>;
export type DevConfig = z.infer<typeof devConfigSchema>;
export type CaidoConfig = z.infer<typeof caidoConfigSchema>; 

export const RootPackageJsonSchema = z.object({
    name: z.string(),
    version: z.string(),
    description: z.string(),
    author: z.object({
        name: z.string(),
        email: z.string().email(),
        url: z.string().url(),
    }),
});

export const PluginPackageJsonSchema = z.object({
    name: z.string(),
});

export type RootPackageJson = z.infer<typeof RootPackageJsonSchema>;
export type PluginPackageJson = z.infer<typeof PluginPackageJsonSchema>;