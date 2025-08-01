import typing as t
from importlib.metadata import version

import httpx

from .client import AsyncTensorZeroGateway, BaseTensorZeroGateway, TensorZeroGateway
from .tensorzero import (
    BestOfNSamplingConfig,
    ChainOfThoughtConfig,
    ChatCompletionConfig,
    Config,
    Datapoint,
    DiclConfig,
    FireworksSFTConfig,
    FunctionConfigChat,
    FunctionConfigJson,
    FunctionsConfig,
    GCPVertexGeminiSFTConfig,
    MixtureOfNConfig,
    OpenAISFTConfig,
    OptimizationJobHandle,
    OptimizationJobInfo,
    OptimizationJobStatus,
    RenderedSample,
    ResolvedInput,
    ResolvedInputMessage,
    StoredInference,
    VariantsConfig,
)
from .tensorzero import (
    _start_http_gateway as _start_http_gateway,
)
from .types import (
    AndFilter,
    AndNode,  # DEPRECATED
    BaseTensorZeroError,
    BooleanMetricFilter,
    BooleanMetricNode,  # DEPRECATED
    ChatDatapointInsert,
    ChatInferenceDatapointInput,  # DEPRECATED
    ChatInferenceResponse,
    ContentBlock,
    DynamicEvaluationRunEpisodeResponse,
    DynamicEvaluationRunResponse,
    ExtraBody,
    FeedbackResponse,
    FileBase64,
    FileUrl,
    FinishReason,
    FloatMetricFilter,
    FloatMetricNode,  # DEPRECATED
    ImageBase64,
    ImageUrl,
    InferenceChunk,
    InferenceInput,
    InferenceResponse,
    JsonDatapointInsert,
    JsonInferenceDatapointInput,  # DEPRECATED
    JsonInferenceOutput,
    JsonInferenceResponse,
    Message,
    NotFilter,
    NotNode,  # DEPRECATED
    OrderBy,
    OrFilter,
    OrNode,  # DEPRECATED
    RawText,
    System,
    TagFilter,
    TensorZeroError,
    TensorZeroInternalError,
    Text,
    TextChunk,
    Thought,
    ThoughtChunk,
    TimeFilter,
    Tool,
    ToolCall,
    ToolCallChunk,
    ToolChoice,
    ToolParams,
    ToolResult,
    UnknownContentBlock,
    Usage,
)

RenderedStoredInference = RenderedSample  # DEPRECATED: use RenderedSample instead
# Type aliases to preserve backward compatibility with main
ChatDatapoint = Datapoint.Chat
JsonDatapoint = Datapoint.Json

OptimizationConfig = t.Union[OpenAISFTConfig, FireworksSFTConfig]
ChatInferenceOutput = t.List[ContentBlock]


__all__ = [
    "AndFilter",
    "AndNode",  # DEPRECATED
    "AsyncTensorZeroGateway",
    "BaseTensorZeroError",
    "BaseTensorZeroGateway",
    "BooleanMetricFilter",
    "BooleanMetricNode",  # DEPRECATED
    "ChatDatapoint",
    "ChatDatapointInsert",
    "ChatInferenceDatapointInput",  # DEPRECATED
    "ChatInferenceResponse",
    "Config",
    "ContentBlock",
    "Datapoint",
    "DynamicEvaluationRunEpisodeResponse",
    "DynamicEvaluationRunResponse",
    "ExtraBody",
    "FeedbackResponse",
    "FileBase64",
    "FileUrl",
    "FinishReason",
    "FloatMetricFilter",
    "FloatMetricNode",  # DEPRECATED
    "FunctionsConfig",
    "FunctionConfigChat",
    "FunctionConfigJson",
    "VariantsConfig",
    "ChatCompletionConfig",
    "BestOfNSamplingConfig",
    "DiclConfig",
    "MixtureOfNConfig",
    "ChainOfThoughtConfig",
    "ImageBase64",
    "ImageUrl",
    "InferenceChunk",
    "ResolvedInput",
    "ResolvedInputMessage",
    "StoredInference",
    "InferenceInput",
    "InferenceResponse",
    "JsonDatapoint",
    "JsonDatapointInsert",
    "JsonInferenceDatapointInput",  # DEPRECATED
    "JsonInferenceOutput",
    "JsonInferenceResponse",
    "Message",
    "NotFilter",
    "NotNode",  # DEPRECATED
    "OrderBy",
    "OrFilter",
    "OrNode",  # DEPRECATED
    "OptimizationJobHandle",
    "OptimizationJobInfo",
    "OptimizationJobStatus",
    "FireworksSFTConfig",
    "GCPVertexGeminiSFTConfig",
    "OpenAISFTConfig",
    "OptimizationConfig",
    "patch_openai_client",
    "RawText",
    "RenderedStoredInference",  # DEPRECATED
    "RenderedSample",
    "System",
    "TagFilter",
    "TensorZeroError",
    "TensorZeroGateway",
    "TensorZeroInternalError",
    "Text",
    "TextChunk",
    "Thought",
    "ThoughtChunk",
    "TimeFilter",
    "Tool",
    "ToolChoice",
    "ToolParams",
    "ToolCall",
    "ToolCallChunk",
    "ToolResult",
    "UnknownContentBlock",
    "Usage",
]

T = t.TypeVar("T", bound=t.Any)

__version__ = version("tensorzero")


def _attach_fields(client: T, gateway: t.Any) -> T:
    if hasattr(client, "__tensorzero_gateway"):
        raise RuntimeError(
            "TensorZero: Already called 'tensorzero.patch_openai_client' on this OpenAI client."
        )
    client.base_url = gateway.base_url
    # Store the gateway so that it doesn't get garbage collected
    client.__tensorzero_gateway = gateway
    return client


async def _async_attach_fields(client: T, awaitable: t.Awaitable[t.Any]) -> T:
    gateway = await awaitable
    return _attach_fields(client, gateway)


class ATTENTION_TENSORZERO_PLEASE_AWAIT_RESULT_OF_PATCH_OPENAI_CLIENT(httpx.URL):
    # This is called by httpx when making a request (to join the base url with the path)
    # We throw an error to try to produce a nicer message for the user
    def copy_with(self, *args: t.Any, **kwargs: t.Any):
        raise RuntimeError(
            "TensorZero: Please await the result of `tensorzero.patch_openai_client` before using the client."
        )


def close_patched_openai_client_gateway(client: t.Any) -> None:
    """
    Closes the TensorZero gateway associated with a patched OpenAI client from `tensorzero.patch_openai_client`
    After calling this function, the patched client becomes unusable

    :param client: The OpenAI client previously patched with `tensorzero.patch_openai_client`
    """
    if hasattr(client, "__tensorzero_gateway"):
        client.__tensorzero_gateway.close()
    else:
        raise ValueError(
            "TensorZero: Called 'close_patched_client_gateway' on an OpenAI client that was not patched with 'tensorzero.patch_openai_client'."
        )


def patch_openai_client(
    client: T,
    *,
    config_file: t.Optional[str] = None,
    clickhouse_url: t.Optional[str] = None,
    async_setup: bool = True,
) -> t.Union[T, t.Awaitable[T]]:
    """
    Starts a new TensorZero gateway, and patching the provided OpenAI client to use it

    :param client: The OpenAI client to patch. This can be an 'OpenAI' or 'AsyncOpenAI' client
    :param config_file: (Optional) The path to the TensorZero configuration file.
    :param clickhouse_url: (Optional) The URL of the ClickHouse database.
    :param async_setup: (Optional) If True, returns an Awaitable that resolves to the patched client once the gateway has started. If False, blocks until the gateway has started.

    :return: The patched OpenAI client, or an Awaitable that resolves to it, depending on the value of `async_setup`
    """
    # If the user passes `async_setup=True`, then they need to 'await' the result of this function for the base_url to set to the running gateway
    # (since we need to await the future for our tensorzero gateway to start up)
    # To prevent requests from getting sent to the real OpenAI server if the user forgets to `await`,
    # we set a fake 'base_url' immediately, which will prevent the client from working until the real 'base_url' is set.
    # This type is set up to (hopefully) produce a nicer error message for the user
    client.base_url = ATTENTION_TENSORZERO_PLEASE_AWAIT_RESULT_OF_PATCH_OPENAI_CLIENT(
        "http://ATTENTION_TENSORZERO_PLEASE_AWAIT_RESULT_OF_PATCH_OPENAI_CLIENT.invalid/"
    )
    gateway = _start_http_gateway(
        config_file=config_file, clickhouse_url=clickhouse_url, async_setup=async_setup
    )
    if async_setup:
        # In 'async_setup' mode, return a `Future` that sets the needed fields after the gateway has started
        return _async_attach_fields(client, gateway)
    return _attach_fields(client, gateway)
