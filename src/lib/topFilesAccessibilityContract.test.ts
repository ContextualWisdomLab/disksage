import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import type { EntryView } from "./api";
import TopFiles from "./TopFiles.svelte";

const sampleFile: EntryView = {
  name: "large.bin",
  path: "/data/large.bin",
  size: 1024,
  is_dir: false,
};

describe("TopFiles accessible data table", () => {
  it("renders the empty result as an announced next action instead of an empty table", () => {
    const { body } = render(TopFiles, { props: { files: [] } });

    expect(body).toContain('<h2 id="top-files-heading">가장 큰 파일 0개</h2>');
    expect(body).toContain('class="empty" role="status"');
    expect(body).toContain("표시할 대용량 파일이 없습니다. 다른 폴더를 선택해 다시 스캔하세요.");
    expect(body).not.toContain("<table");
  });

  it("renders a named table with explicit column headers", () => {
    const { body } = render(TopFiles, { props: { files: [sampleFile] } });

    expect(body).toContain('<h2 id="top-files-heading">가장 큰 파일 1개</h2>');
    expect(body).toContain('<table aria-labelledby="top-files-heading">');
    expect(body).toContain('<th scope="col">크기</th>');
    expect(body).toContain('<th scope="col">경로</th>');
    expect(body).toContain("/data/large.bin");
  });

  it("renders a sequential fragment link to the named keyboard-scroll target", () => {
    const { body } = render(TopFiles, { props: { files: [sampleFile] } });

    expect(body).toContain('<a class="table-focus" href="#top-files-table">파일 표 탐색 시작</a>');
    expect(body).toContain('id="top-files-table"');
    expect(body).toContain('class="table-scroll"');
    expect(body).toContain('role="region"');
    expect(body).toContain('tabindex="-1"');
    expect(body).toContain('aria-labelledby="top-files-heading"');
  });
});
