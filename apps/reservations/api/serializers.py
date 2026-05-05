from rest_framework import serializers
from apps.reservations.models import Reservation
class ReservationSerializer(serializers.ModelSerializer):
    class Meta:
        model=Reservation
        fields='__all__'
