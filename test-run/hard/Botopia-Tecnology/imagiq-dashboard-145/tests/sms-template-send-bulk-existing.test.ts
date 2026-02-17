import assert from "node:assert/strict";
import { smsTemplateEndpoints, apiClient } from "../src/lib/api";

async function run() {
  const originalPost = apiClient.post.bind(apiClient);

  try {
    let capturedEndpoint: string | undefined;
    let capturedData: any;

    apiClient.post = async (endpoint: string, data?: unknown) => {
      capturedEndpoint = endpoint;
      capturedData = data;
      return {
        success: true,
        data: { success: true, message: "ok" },
      } as any;
    };

    const recipients = [
      { phoneNumber: "5550001111", variables: { name: "Ana" } },
      { phoneNumber: "5550002222", variables: { name: "Luis" } },
    ];

    await smsTemplateEndpoints.sendBulk("template-xyz", recipients);

    assert.equal(
      capturedEndpoint,
      "/api/messaging/sms-templates/template-xyz/send-bulk"
    );
    assert.deepEqual(capturedData, { recipients });
  } finally {
    apiClient.post = originalPost;
  }
}

run().catch((error) => {
  console.error(error);
  process.exit(1);
});
