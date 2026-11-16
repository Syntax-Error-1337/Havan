export const qk = {
  products: ['products'] as const,
  product: (id: string) => ['product', id] as const,
  validation: (id: string) => ['product', id, 'validation'] as const,
  firmware: (productId: string) => ['firmware', productId] as const,
  campaigns: ['campaigns'] as const,
  apiKeys: ['api-keys'] as const,
  licences: ['licences'] as const,
  team: ['team'] as const,
  invoices: ['invoices'] as const,
  webhooks: ['webhooks'] as const,
  security: ['security'] as const,
};
