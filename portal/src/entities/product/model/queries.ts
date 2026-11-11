import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Product, ThingModel, Validation } from './types';

export function useProducts() {
  return useQuery({
    queryKey: qk.products,
    queryFn: () => request<Product[]>('/vendor/products'),
  });
}

export function useProduct(id: string | undefined) {
  return useQuery({
    queryKey: qk.product(id ?? 'none'),
    enabled: Boolean(id),
    queryFn: () => request<Product>(`/vendor/products/${id}`),
  });
}

export function useCreateProduct() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { thing_model: ThingModel; publish: boolean }) =>
      request<Product>('/vendor/products', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.products }),
  });
}

export function useUpdateProduct(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { thing_model: ThingModel }) =>
      request<Product>(`/vendor/products/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(input),
      }),
    onSuccess: (product) => {
      qc.setQueryData(qk.product(id), product);
      qc.invalidateQueries({ queryKey: qk.products });
    },
  });
}

/** Returns violations instead of throwing: the editor renders them inline. */
export function useValidateProduct(id: string) {
  return useMutation({
    mutationFn: () =>
      request<Validation>(`/vendor/products/${id}/validate`, { method: 'POST' }),
  });
}

export function usePublishProduct(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () =>
      request<Product>(`/vendor/products/${id}/publish`, { method: 'POST' }),
    onSuccess: (product) => {
      qc.setQueryData(qk.product(id), product);
      qc.invalidateQueries({ queryKey: qk.products });
    },
  });
}
