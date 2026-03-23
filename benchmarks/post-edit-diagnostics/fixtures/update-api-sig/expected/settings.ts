import { fetchUser } from "./api";

export async function loadSettings(userId: number): Promise<Record<string, string>> {
  const user = await fetchUser({ id: userId });
  return {
    name: user.name,
    email: user.email,
    bio: user.profile?.bio ?? "No bio set",
  };
}

export async function getEmail(userId: number): Promise<string> {
  const user = await fetchUser({ id: userId });
  return user.email;
}
