import { test } from "@playwright/test";
import { flow } from "./shots";

test.use({ viewport: { width: 390, height: 844 } });
flow("phone", true);
