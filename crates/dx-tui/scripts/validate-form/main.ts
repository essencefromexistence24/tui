import * as core from "@actions/core";
import * as actionsGithub from "@actions/github";
import { pathToFileURL } from "node:url";

const LABEL_NAME = "needs info";
const RE_VERSION = /dx\s+Version\s*:\s\d+\.\d+\.\d+\s\(/m;
const RE_DEPENDENCIES = /Dependencies\s+[/a-z]+\s*:/m;
const RE_CHECKLIST = /#{3}\s+Checklist\s+(?:^-\s+\[x]\s+.+?(?:\n|\r\n|$)){2}/m;

type GitHubClient = ReturnType<typeof actionsGithub.getOctokit>;
type GitHubContext = typeof actionsGithub.context;

type ActionServices = {
  github: GitHubClient;
  context: GitHubContext;
  core: typeof core;
};

type IssuePayload = {
  action?: string;
  issue?: {
    number: number;
    body?: string | null;
    created_at: string;
    user?: {
      login: string;
    } | null;
  };
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function bugReportBody(creator: string, content: string, hash: string): string | null {
  const hasCurrentDebugInfo =
    RE_DEPENDENCIES.test(content) &&
    RE_CHECKLIST.test(content) &&
    new RegExp(` \\(${hash}[a-f0-9]? `).test(content);

  if (hasCurrentDebugInfo) {
    return null;
  }

  return `Hey @${creator}, thank you for opening the issue to help improve dx, appreciate it!

I noticed that you did not correctly follow the issue template. Please ensure that:

- The bug can still be reproduced on the [newest nightly build](https://dx-rs.github.io/docs/installation/#binaries).
- The debug information (\`dx --debug\`) is updated for the newest nightly.
- The non-optional items in the checklist are checked.

Issues with \`${LABEL_NAME}\` will be marked ready once edited with the proper content, or closed after 2 days of inactivity.

Our maintainers work on dx in their free time, this helps them work efficiently, understand your setup quickly, and find a more appropriate solution. Thanks for your understanding! 🙏
`;
}

export function featureRequestBody(creator: string, content: string): string | null {
  if (RE_VERSION.test(content) && RE_DEPENDENCIES.test(content) && RE_CHECKLIST.test(content)) {
    return null;
  }

  return `Hey @${creator}, thank you for opening the issue to help improve dx, appreciate it!

I noticed that you did not correctly follow the issue template. Please ensure that:

- The requested feature does not exist in the [newest nightly build](https://dx-rs.github.io/docs/installation/#binaries).
- The debug information (\`dx --debug\`) is updated for the newest nightly.
- The non-optional items in the checklist are checked.

Issues with \`${LABEL_NAME}\` will be marked ready once edited with the proper content, or closed after 2 days of inactivity.

Our maintainers work on dx in their free time, this helps them work efficiently, understand your setup quickly, and find a more appropriate solution. Thanks for your understanding! 🙏
`;
}

export async function validateForm({ github, context, core }: ActionServices): Promise<void> {
  async function nightlyHash(): Promise<string | null> {
    try {
      const { data: tagRef } = await github.rest.git.getRef({
        owner: "sxdx",
        repo: "dx",
        ref: "tags/nightly",
      });

      return tagRef.object.sha.slice(0, 7);
    } catch (error) {
      if (typeof error === "object" && error !== null && "status" in error && error.status === 404) {
        core.error("Nightly tag not found");
      } else {
        core.error(`Error fetching nightly version: ${errorMessage(error)}`);
      }

      return null;
    }
  }

  async function hasLabel(id: number, label: string): Promise<boolean> {
    try {
      const { data: labels } = await github.rest.issues.listLabelsOnIssue({
        ...context.repo,
        issue_number: id,
      });

      return labels.some((item) => item.name === label);
    } catch (error) {
      core.error(`Error checking labels: ${errorMessage(error)}`);
      return false;
    }
  }

  async function lastLabeledAt(id: number): Promise<string | null> {
    try {
      const { data: events } = await github.rest.issues.listEvents({
        ...context.repo,
        issue_number: id,
        per_page: 100,
      });

      const labeledEvents = events.filter(
        (event) => event.event === "labeled" && event.label?.name === LABEL_NAME,
      );

      return labeledEvents.at(-1)?.created_at ?? null;
    } catch (error) {
      core.error(`Error getting label timestamp: ${errorMessage(error)}`);
      return null;
    }
  }

  async function removedLabelManually(id: number): Promise<boolean> {
    try {
      const { data: events } = await github.rest.issues.listEvents({
        ...context.repo,
        issue_number: id,
        per_page: 100,
      });

      const unlabeledEvents = events.filter(
        (event) => event.event === "unlabeled" && event.label?.name === LABEL_NAME,
      );
      const lastActor = unlabeledEvents.at(-1)?.actor?.login ?? "";

      return unlabeledEvents.length > 0 && !lastActor.endsWith("[bot]");
    } catch (error) {
      core.error(`Error checking label removal history: ${errorMessage(error)}`);
      return false;
    }
  }

  async function updateLabels(id: number, mark: boolean, body: string | null): Promise<void> {
    try {
      const marked = await hasLabel(id, LABEL_NAME);

      if (!mark && marked) {
        await github.rest.issues.removeLabel({
          ...context.repo,
          issue_number: id,
          name: LABEL_NAME,
        });
        await hideOldComments(id);
      } else if (mark && body && !marked && !(await removedLabelManually(id))) {
        await github.rest.issues.addLabels({
          ...context.repo,
          issue_number: id,
          labels: [LABEL_NAME],
        });
        await hideOldComments(id);
        await github.rest.issues.createComment({
          ...context.repo,
          issue_number: id,
          body,
        });
      }
    } catch (error) {
      core.error(`Error updating labels: ${errorMessage(error)}`);
    }
  }

  async function hideOldComments(id: number): Promise<void> {
    try {
      const comments = await github.paginate(github.rest.issues.listComments, {
        ...context.repo,
        issue_number: id,
        per_page: 100,
      });

      for (const comment of comments) {
        const byBot = comment.user?.login?.endsWith("[bot]") || comment.user?.type === "Bot";
        const contains = comment.body?.includes("or closed after 2 days of inactivity");
        if (!byBot || !contains || !comment.node_id) {
          continue;
        }

        try {
          await github.graphql(
            `mutation($subjectId: ID!, $classifier: ReportedContentClassifiers!) {
              minimizeComment(input: {subjectId: $subjectId, classifier: $classifier}) {
                minimizedComment { isMinimized }
              }
            }`,
            { subjectId: comment.node_id, classifier: "OUTDATED" },
          );
        } catch (error) {
          core.error(`Error minimizing comment ${comment.id}: ${errorMessage(error)}`);
        }
      }
    } catch (error) {
      core.error(`Error listing comments: ${errorMessage(error)}`);
    }
  }

  async function closeOldIssues(): Promise<void> {
    try {
      const { data: issues } = await github.rest.issues.listForRepo({
        ...context.repo,
        state: "open",
        labels: LABEL_NAME,
      });

      const twoDaysAgo = new Date(Date.now() - 2 * 24 * 60 * 60 * 1000);

      for (const issue of issues) {
        const markedAt = new Date((await lastLabeledAt(issue.number)) || issue.created_at);
        if (markedAt >= twoDaysAgo) {
          continue;
        }

        await github.rest.issues.update({
          ...context.repo,
          issue_number: issue.number,
          state: "closed",
          state_reason: "not_planned",
        });
        await github.rest.issues.createComment({
          ...context.repo,
          issue_number: issue.number,
          body: `This issue has been automatically closed because it was marked as \`${LABEL_NAME}\` for more than 2 days without updates.
If the problem persists, please file a new issue and complete the issue template so we can capture all the details necessary to investigate further.`,
        });
      }
    } catch (error) {
      core.error(`Error checking old issues: ${errorMessage(error)}`);
    }
  }

  async function closeUnsupportedIssue(id: number): Promise<void> {
    try {
      await github.rest.issues.update({
        ...context.repo,
        issue_number: id,
        state: "closed",
        state_reason: "not_planned",
      });
      await github.rest.issues.createComment({
        ...context.repo,
        issue_number: id,
        body: `Unsupported issue template.
Either the [Bug Report](https://github.com/sxdx/dx/issues/new?template=bug.yml) or [Feature Request](https://github.com/sxdx/dx/issues/new?template=feature.yml) template should be used.`,
      });
    } catch (error) {
      core.error(`Error closing unsupported issue: ${errorMessage(error)}`);
    }
  }

  const hash = await nightlyHash();
  if (!hash) {
    return;
  }

  if (context.eventName === "schedule") {
    await closeOldIssues();
    return;
  }

  if (context.eventName !== "issues") {
    return;
  }

  const payload = context.payload as IssuePayload;
  const issue = payload.issue;
  if (!issue?.user?.login) {
    core.error("Issue event is missing the issue payload.");
    return;
  }

  if (await hasLabel(issue.number, "bug")) {
    const body = bugReportBody(issue.user.login, issue.body || "", hash);
    await updateLabels(issue.number, !!body, body);
  } else if (await hasLabel(issue.number, "feature")) {
    const body = featureRequestBody(issue.user.login, issue.body || "");
    await updateLabels(issue.number, !!body, body);
  } else if (payload.action === "opened") {
    await closeUnsupportedIssue(issue.number);
  }
}

async function run(): Promise<void> {
  const token = process.env.GITHUB_TOKEN;
  if (!token) {
    throw new Error("GITHUB_TOKEN is required.");
  }

  await validateForm({
    github: actionsGithub.getOctokit(token),
    context: actionsGithub.context,
    core,
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    await run();
  } catch (error) {
    core.setFailed(errorMessage(error));
  }
}
