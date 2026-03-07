use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post, put},
};

use env_logger::Env;
use log::error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Product {
    id: u32,
    name: String,
    description: String,
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NewProduct {
    name: String,
    description: String,
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProductForUpd {
    name: Option<String>,
    description: Option<String>,
    image: Option<String>,
}

#[derive(Clone)]
struct AppState {
    products: Arc<Mutex<HashMap<u32, Product>>>,
    next_id: Arc<Mutex<u32>>,
}

fn get_and_set_next_id(state: &AppState) -> u32 {
    let mut next_id = state.next_id.lock().unwrap();
    let id = *next_id;
    *next_id += 1;
    id
}

async fn create_product(
    State(state): State<AppState>,
    Json(new_product): Json<NewProduct>,
) -> impl IntoResponse {
    let product = Product {
        id: get_and_set_next_id(&state),
        name: new_product.name,
        description: new_product.description,
        image: new_product.image,
    };
    state
        .products
        .lock()
        .unwrap()
        .insert(product.id, product.clone());

    (StatusCode::CREATED, Json(product))
}

async fn get_product(State(state): State<AppState>, Path(id): Path<u32>) -> impl IntoResponse {
    match state.products.lock().unwrap().get(&id) {
        Some(product) => (StatusCode::OK, Json(product)).into_response(),
        _ => (
            StatusCode::NOT_FOUND,
            format!("Product with id = {} was not found", id),
        )
            .into_response(),
    }
}

async fn upd_product(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Json(product_for_upd): Json<ProductForUpd>,
) -> impl IntoResponse {
    if let Some(product) = state.products.lock().unwrap().get_mut(&id) {
        if let Some(name) = product_for_upd.name {
            product.name = name;
        }
        if let Some(description) = product_for_upd.description {
            product.description = description;
        }
        if product_for_upd.image.is_some() {
            product.image = product_for_upd.image;
        }

        (StatusCode::OK, Json(product)).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            format!("Product with id = {} was not found", id),
        )
            .into_response()
    }
}

async fn delete_product(State(state): State<AppState>, Path(id): Path<u32>) -> impl IntoResponse {
    match state.products.lock().unwrap().remove(&id) {
        Some(product) => {
            if let Some(path) = &product.image {
                let _ = fs::remove_file(path);
            }
            (StatusCode::OK, Json(product)).into_response()
        }
        _ => (
            StatusCode::NOT_FOUND,
            format!("Product with id = {} was not found", id),
        )
            .into_response(),
    }
}

async fn get_products(State(state): State<AppState>) -> impl IntoResponse {
    let products: Vec<Product> = state.products.lock().unwrap().values().cloned().collect();
    Json(products)
}

async fn upload_image(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if !state.products.lock().unwrap().contains_key(&id) {
        return (
            StatusCode::NOT_FOUND,
            format!("Product with id = {} was not found", id),
        )
            .into_response();
    }

    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() != Some("icon") {
            continue;
        }

        let data = match field.bytes().await {
            Ok(d) => d.to_vec(),
            Err(err) => {
                error!("Error during reading file: {}", err);
                return (StatusCode::BAD_REQUEST, "Error during reading file").into_response();
            }
        };

        let dir = "./images";
        let path = format!("{}/{}.png", dir, id);
        fs::create_dir_all(dir).unwrap();
        if let Err(err) = fs::write(&path, &data) {
            error!("Error during saving file: {}", err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error during saving file",
            )
                .into_response();
        }
        if let Some(product) = state.products.lock().unwrap().get_mut(&id) {
            product.image = Some(path);
            return (StatusCode::OK, Json(product)).into_response();
        }
        return (
            StatusCode::NOT_FOUND,
            format!("Product with id = {} was not found", id),
        )
            .into_response();
    }
    (StatusCode::BAD_REQUEST, "Field image was not found").into_response()
}

async fn get_image(State(state): State<AppState>, Path(id): Path<u32>) -> impl IntoResponse {
    if let Some(product) = state.products.lock().unwrap().get(&id) {
        if let Some(path) = &product.image {
            match fs::read(path) {
                Ok(data) => {
                    let headers = [(header::CONTENT_TYPE, "image/png")];
                    return (headers, data).into_response();
                }
                Err(err) => {
                    error!(
                        "Error during reading image for product with id = {}: {}",
                        id, err
                    );
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Error during reading image for product with id = {}", id),
                    )
                        .into_response();
                }
            }
        }
    }
    (
        StatusCode::NOT_FOUND,
        format!("Image for product with id = {} was not found", id),
    )
        .into_response()
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    let state = AppState {
        products: Arc::new(Mutex::new(HashMap::new())),
        next_id: Arc::new(Mutex::new(1)),
    };
    let app: Router = Router::new()
        .route("/product", post(create_product))
        .route("/product/{id}", get(get_product))
        .route("/product/{id}", put(upd_product))
        .route("/product/{id}", delete(delete_product))
        .route("/products", get(get_products))
        .route("/product/{id}/image", post(upload_image))
        .route("/product/{id}/image", get(get_image))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
