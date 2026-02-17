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
        data: { status: "ok", message: "queued", estimatedTotal: 25 },
      } as any;
    };

    const extraPhones = ["5551234567", "5559876543"];
    const maxRecipients = 42;
    const productFilter = { categoria: "smartphones" };

    await smsTemplateEndpoints.sendToAll(
      "template-abc",
      maxRecipients,
      productFilter,
      extraPhones
    );

    assert.equal(
      capturedEndpoint,
      "/api/messaging/sms-templates/template-abc/send-to-all-sms"
    );
    assert.deepEqual(capturedData, {
      maxRecipients,
      productFilter,
      extraPhones,
    });

    apiClient.post = async (endpoint: string, data?: unknown) => {
      capturedEndpoint = endpoint;
      capturedData = data;
      return {
        success: true,
        data: { status: "ok", message: "queued", estimatedTotal: 10 },
      } as any;
    };

    await smsTemplateEndpoints.sendToAll(
      "template-def",
      undefined,
      undefined,
      undefined
    );

    assert.equal(
      capturedEndpoint,
      "/api/messaging/sms-templates/template-def/send-to-all-sms"
    );
    assert.deepEqual(capturedData, {
      maxRecipients: undefined,
      productFilter: undefined,
      extraPhones: undefined,
    });
  } finally {
    apiClient.post = originalPost;
  }
}

run().catch((error) => {
  console.error(error);
  process.exit(1);
});
