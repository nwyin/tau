import { fetchUser, User } from "./api";

export async function renderDashboard(userId: number): Promise<string> {
  const user: User = await fetchUser(userId);
  return `<h1>Welcome, ${user.name}</h1><p>${user.email}</p>`;
}

export async function getUserName(id: number): Promise<string> {
  const user = await fetchUser(id);
  return user.name;
}
