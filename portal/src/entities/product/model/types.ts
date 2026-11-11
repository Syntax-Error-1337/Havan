export type RuleViolation = {
  rule: number;
  path: string;
  message: string;
};

export type Product = {
  id: string;
  vendor_id: string;
  product_key: string;
  name: string;
  category: string;
  status: 'draft' | 'published' | 'deprecated';
  schema_version: number;
  thing_model: ThingModel;
  created_at: string;
};

/** The subset the editor renders as fields; the rest stays JSON. */
export type ThingModel = {
  schema_version: number;
  product_key: string;
  name: string;
  category: string;
  capabilities: Array<{
    name: string;
    points: Record<string, { dp: string; kind: string }>;
  }>;
  constraints?: {
    command_timeout_ms?: number;
    min_firmware?: string;
    allow_unknown_serial?: boolean;
  };
};

export type Validation = {
  valid: boolean;
  violations: RuleViolation[];
};
