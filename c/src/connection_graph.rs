use crate::{FoxgloveError, FoxgloveString, result_to_c};

pub struct FoxgloveConnectionGraph(pub(crate) foxglove::websocket::ConnectionGraph);

impl Default for FoxgloveConnectionGraph {
    fn default() -> Self {
        Self(foxglove::websocket::ConnectionGraph::new())
    }
}

/// Create a new connection graph.
///
/// The graph must later be freed with `foxglove_connection_graph_free`.
///
/// # Safety
/// `graph` must be a valid pointer to a pointer to a `foxglove_connection_graph`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_connection_graph_create(
    graph: *mut *mut FoxgloveConnectionGraph,
) -> FoxgloveError {
    let graph_box = Box::new(FoxgloveConnectionGraph::default());
    if graph.is_null() {
        return FoxgloveError::ValueError;
    }

    unsafe { *graph = Box::into_raw(graph_box) };

    FoxgloveError::Ok
}

/// Free the connection graph.
///
/// # Safety
/// `graph` must be a valid pointer to a `foxglove_connection_graph` created by
/// `foxglove_connection_graph_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_connection_graph_free(graph: *mut FoxgloveConnectionGraph) {
    drop(unsafe { Box::from_raw(graph) });
}

/// Set a published topic and its associated publisher ids. Overwrites any existing topic with the
/// same name.
///
/// # Safety
/// `topic`, and each ID in `publisher_ids` must adhere to the safety rules of `foxglove_string`.
/// `publisher_ids_count` must be the number of elements in the `publisher_ids` array.
///
/// These strings are copied from the pointers, and need only be valid for the duration of this
/// function call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_connection_graph_set_published_topic(
    graph: &mut FoxgloveConnectionGraph,
    topic: FoxgloveString,
    publisher_ids: *const FoxgloveString,
    publisher_ids_count: usize,
) -> FoxgloveError {
    let result = unsafe {
        do_foxglove_connection_graph_set_published_topic(
            graph,
            topic,
            publisher_ids,
            publisher_ids_count,
        )
    };
    unsafe { result_to_c(result, std::ptr::null_mut()) }
}

unsafe fn do_foxglove_connection_graph_set_published_topic(
    graph: &mut FoxgloveConnectionGraph,
    topic: FoxgloveString,
    publisher_ids: *const FoxgloveString,
    publisher_ids_count: usize,
) -> Result<(), foxglove::FoxgloveError> {
    let topic = unsafe { topic.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("topic is invalid: {e}")))?;

    if publisher_ids_count > 0 {
        if publisher_ids.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "publisher_ids is null".to_string(),
            ));
        }

        let mut strings: Vec<&str> = Vec::with_capacity(publisher_ids_count);

        for publisher_id in
            unsafe { std::slice::from_raw_parts(publisher_ids, publisher_ids_count) }
        {
            if publisher_id.data.is_null() {
                return Err(foxglove::FoxgloveError::ValueError(
                    "encountered a null publisher_id".to_string(),
                ));
            }
            let id = unsafe { publisher_id.as_utf8_str() }.map_err(|e| {
                foxglove::FoxgloveError::Utf8Error(format!("publisher_id is invalid: {e}"))
            })?;
            strings.push(id);
        }

        graph.0.set_published_topic(topic, strings);
    }
    Ok(())
}

/// Set a subscribed topic and its associated subscriber ids. Overwrites any existing topic with the
/// same name.
///
/// # Safety
/// `topic`, and each ID in `subscriber_ids` must adhere to the safety rules of `foxglove_string`.
/// `subscriber_ids_count` must be the number of elements in the `subscriber_ids` array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_connection_graph_set_subscribed_topic(
    graph: &mut FoxgloveConnectionGraph,
    topic: FoxgloveString,
    subscriber_ids: *const FoxgloveString,
    subscriber_ids_count: usize,
) -> FoxgloveError {
    let result = unsafe {
        do_foxglove_connection_graph_set_subscribed_topic(
            graph,
            topic,
            subscriber_ids,
            subscriber_ids_count,
        )
    };
    unsafe { result_to_c(result, std::ptr::null_mut()) }
}

unsafe fn do_foxglove_connection_graph_set_subscribed_topic(
    graph: &mut FoxgloveConnectionGraph,
    topic: FoxgloveString,
    subscriber_ids: *const FoxgloveString,
    subscriber_ids_count: usize,
) -> Result<(), foxglove::FoxgloveError> {
    let topic = unsafe { topic.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("topic is invalid: {e}")))?;

    if subscriber_ids_count > 0 {
        if subscriber_ids.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "subscriber_ids is null".to_string(),
            ));
        }

        let mut strings: Vec<&str> = Vec::with_capacity(subscriber_ids_count);

        for subscriber_id in
            unsafe { std::slice::from_raw_parts(subscriber_ids, subscriber_ids_count) }
        {
            if subscriber_id.data.is_null() {
                return Err(foxglove::FoxgloveError::ValueError(
                    "encountered a null subscriber_id".to_string(),
                ));
            }
            let id = unsafe { subscriber_id.as_utf8_str() }.map_err(|e| {
                foxglove::FoxgloveError::Utf8Error(format!("subscriber_id is invalid: {e}"))
            })?;
            strings.push(id);
        }

        graph.0.set_subscribed_topic(topic, strings);
    }
    Ok(())
}

/// Set an advertised service and its associated provider ids. Overwrites any existing service with
/// the same name.
///
/// # Safety
/// `graph` must be a valid pointer to a `foxglove_connection_graph` created by
/// `foxglove_connection_graph_create`. `service`, and each ID in `provider_ids` must adhere to the
/// safety rules of `FoxgloveString`. `provider_ids_count` must be the number of elements in the
/// `provider_ids` array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_connection_graph_set_advertised_service(
    graph: &mut FoxgloveConnectionGraph,
    service: FoxgloveString,
    provider_ids: *const FoxgloveString,
    provider_ids_count: usize,
) -> FoxgloveError {
    let result = unsafe {
        do_foxglove_connection_graph_set_advertised_service(
            graph,
            service,
            provider_ids,
            provider_ids_count,
        )
    };
    unsafe { result_to_c(result, std::ptr::null_mut()) }
}

unsafe fn do_foxglove_connection_graph_set_advertised_service(
    graph: &mut FoxgloveConnectionGraph,
    service: FoxgloveString,
    provider_ids: *const FoxgloveString,
    provider_ids_count: usize,
) -> Result<(), foxglove::FoxgloveError> {
    let service = unsafe { service.as_utf8_str() }
        .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("service is invalid: {e}")))?;

    if provider_ids_count > 0 {
        if provider_ids.is_null() {
            return Err(foxglove::FoxgloveError::ValueError(
                "provider_ids is null".to_string(),
            ));
        }

        let mut strings: Vec<&str> = Vec::with_capacity(provider_ids_count);

        for provider_id in unsafe { std::slice::from_raw_parts(provider_ids, provider_ids_count) } {
            if provider_id.data.is_null() {
                return Err(foxglove::FoxgloveError::ValueError(
                    "encountered a null provider_id".to_string(),
                ));
            }
            let id = unsafe { provider_id.as_utf8_str() }.map_err(|e| {
                foxglove::FoxgloveError::Utf8Error(format!("provider_id is invalid: {e}"))
            })?;
            strings.push(id);
        }

        graph.0.set_advertised_service(service, strings);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_published_topic() {
        let mut graph = FoxgloveConnectionGraph(foxglove::websocket::ConnectionGraph::new());
        let publisher_ids = ["publisher_id_1".into(), "publisher_id_2".into()];

        let result = unsafe {
            foxglove_connection_graph_set_published_topic(
                &mut graph,
                "topic".into(),
                publisher_ids.as_ptr(),
                publisher_ids.len(),
            )
        };
        assert_eq!(result, FoxgloveError::Ok);
    }

    #[test]
    fn test_set_subscribed_topic() {
        let mut graph = FoxgloveConnectionGraph(foxglove::websocket::ConnectionGraph::new());
        let subscriber_ids = ["subscriber_id_1".into(), "subscriber_id_2".into()];

        let result = unsafe {
            foxglove_connection_graph_set_subscribed_topic(
                &mut graph,
                "topic".into(),
                subscriber_ids.as_ptr(),
                subscriber_ids.len(),
            )
        };
        assert_eq!(result, FoxgloveError::Ok);
    }

    #[test]
    fn test_set_advertised_service() {
        let mut graph = FoxgloveConnectionGraph(foxglove::websocket::ConnectionGraph::new());
        let provider_ids = ["provider_id_1".into(), "provider_id_2".into()];

        let result = unsafe {
            foxglove_connection_graph_set_advertised_service(
                &mut graph,
                "service".into(),
                provider_ids.as_ptr(),
                provider_ids.len(),
            )
        };
        assert_eq!(result, FoxgloveError::Ok);
    }
}
