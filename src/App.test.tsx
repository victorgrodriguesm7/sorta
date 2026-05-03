import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import "./i18n";

describe("App", () => {
  it("renders the localized title", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: /sorta/i })).toBeInTheDocument();
  });
});
