import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { AppForm } from "./AppForm";

describe("AppForm", () => {
  it("changes behavior defaults without replacing the chosen program", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<AppForm onSave={onSave} onClose={vi.fn()} />);

    fireEvent.change(screen.getByLabelText("App name"), { target: { value: "Local API" } });
    fireEvent.change(screen.getByLabelText("Executable file"), { target: { value: "C:\\Tools\\api.exe" } });
    fireEvent.click(screen.getByRole("button", { name: /Custom/ }));

    expect(screen.getByLabelText("App name")).toHaveValue("Local API");
    expect(screen.getByLabelText("Executable file")).toHaveValue("C:\\Tools\\api.exe");
    const toggles = screen.getAllByRole("checkbox");
    expect(toggles[0]).not.toBeChecked();
    expect(toggles[1]).not.toBeChecked();
    expect(toggles[2]).toBeChecked();
    expect(toggles[3]).toBeChecked();
  });

  it("starts new apps immediately by default", () => {
    render(<AppForm onSave={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByRole("checkbox", { name: /Start after creating/ })).toBeChecked();
  });
});
