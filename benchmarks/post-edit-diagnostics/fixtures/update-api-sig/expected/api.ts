export interface User {
  id: number;
  name: string;
  email: string;
  profile?: {
    bio: string;
    avatar: string;
  };
}

export async function fetchUser(opts: { id: number; includeProfile?: boolean }): Promise<User> {
  const response = await fetch(`/api/users/${opts.id}`);
  if (!response.ok) {
    throw new Error(`Failed to fetch user ${opts.id}`);
  }
  return response.json();
}

export async function deleteUser(id: number): Promise<void> {
  await fetch(`/api/users/${id}`, { method: "DELETE" });
}
