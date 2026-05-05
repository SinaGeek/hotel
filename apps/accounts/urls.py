from django.urls import path
from . import views

urlpatterns = [
    path('profile/', views.profile_view, name='profile'),
    path('login/', views.login_view, name='login'),
    path('logout/', views.logout_view, name='logout'),
    path('register/', views.register_view, name='register'),
    path('activate/<int:user_id>/', views.activate_user_view, name='activate_user'),
    path('reject/<int:user_id>/', views.reject_user_view, name='reject_user'),
]
