export interface User {
  id: number;
  name: string;
  email: string;
  profile?: {
    bio: string;
    avatar: string;
  };
}

export async function fetchUser(id: number): Promise<User> {
  const response = await fetch(`/api/users/${id}`);
  if (!response.ok) {
    throw new Error(`Failed to fetch user ${id}`);
  }
  return response.json();
}

export async function deleteUser(id: number): Promise<void> {
  await fetch(`/api/users/${id}`, { method: "DELETE" });
}
