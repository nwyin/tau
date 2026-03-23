The `fetchUser` function in `api.ts` currently takes a single `id: number`
parameter. Change its signature to accept an options object instead:

```ts
fetchUser(opts: { id: number; includeProfile?: boolean }): Promise<User>
```

Update both callers in `dashboard.ts` and `settings.ts` to use the new
signature. Make sure all files compile with `tsc --noEmit`.
