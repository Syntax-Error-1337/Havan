import { useMutation } from '@tanstack/react-query';
import { request } from '../../shared/api/client';

export type Tokens = { access: string; refresh: string; expires_in: number };

export function useLogin() {
  return useMutation({
    mutationFn: (input: { email: string; password: string }) =>
      request<Tokens>('/auth/login', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
  });
}

export function useRegister() {
  return useMutation({
    mutationFn: (input: { email: string; password: string }) =>
      request<Tokens>('/auth/register', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
  });
}
