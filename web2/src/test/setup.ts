import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// Without the globals of vitest the library does not unmount by itself.
afterEach(cleanup);
